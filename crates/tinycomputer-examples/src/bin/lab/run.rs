//! One trial: running a scenario's flow or goal, and keeping `--send` addressed
//! only to the operator.

use std::{
    io,
    path::{Path, PathBuf},
    time::Instant,
};

use tinycomputer_bus::{
    Deliberation, FLOW_GUIDE, Flow, FlowAction, FlowLoop, FlowRunResult, FlowStep, FlowStrategy,
    RunFlowRequest, RunGoalRequest,
};
use tinycomputer_examples::lab::{
    host::{Host, HostOptions, LabError, module_path},
    record::{
        RunRecord, flow_timeline, goal_timeline, load_memory, report, run_dir, save_memory,
        scorecard, write_json,
    },
    scenario::{SCENARIOS, Scenario, find},
};
use super::{MEMORY, Options, RUNS, authored, run_id};

async fn run_once(
    host: &Host,
    scenario: &Scenario,
    mode: &str,
    options: &Options,
) -> Result<RunRecord, LabError> {
    let dir = run_dir(Path::new(RUNS), scenario.name, mode)?;
    let run = run_id();
    scenario.prepare(host).await?;
    let started = Instant::now();
    let mut llm_calls = 0;
    let (last, stop, actions, jev_calls) = match mode {
        "goal" => {
            let request = RunGoalRequest {
                app: scenario.app.to_owned(),
                goal: scenario.goal.replace("{run}", &run),
                text: scenario
                    .texts
                    .iter()
                    .map(|text| text.replace("{run}", &run))
                    .collect(),
                include_values: true,
                ..RunGoalRequest::default()
            };
            write_json(&dir, "request.json", &request)?;
            let result = host.run_goal(&request).await?;
            write_json(&dir, "result.json", &result)?;
            let timeline = goal_timeline(&result);
            std::fs::write(dir.join("timeline.txt"), &timeline)?;
            println!("{timeline}");
            let actions = u32::try_from(result.turns.len()).unwrap_or(u32::MAX);
            (
                None,
                format!("{:?}", result.stop),
                actions,
                result.metrics.calls,
            )
        }
        "authored" => authored(host, scenario, options, &dir, &run, &mut llm_calls).await?,
        _ => {
            let flow = match &options.flow_file {
                Some(path) => serde_json::from_str(&std::fs::read_to_string(path)?)?,
                None => scenario.flow_json()?,
            };
            let result = run_flow(host, flow, options, &dir, "", &run, mode).await?;
            let summary = (result.stop, result.actions, result.metrics.calls);
            (
                Some(result),
                format!("{:?}", summary.0),
                summary.1,
                summary.2,
            )
        }
    };
    let verdict = scenario.verdict(host, last.as_ref(), &run).await?;
    let record = RunRecord {
        scenario: scenario.name.to_owned(),
        mode: mode.to_owned(),
        passed: verdict.passed,
        detail: verdict.detail,
        stop,
        actions,
        jev_calls,
        llm_calls,
        seconds: started.elapsed().as_secs_f64(),
        dir: dir.clone(),
    };
    write_json(&dir, "verdict.json", &record)?;
    println!(
        "checker: {} — {}\nartifacts: {}",
        if record.passed { "PASS" } else { "FAIL" },
        record.detail,
        dir.display()
    );
    Ok(record)
}

async fn run_flow(
    host: &Host,
    flow: serde_json::Value,
    options: &Options,
    dir: &Path,
    suffix: &str,
    run: &str,
    mode: &str,
) -> Result<FlowRunResult, LabError> {
    let mut request = RunFlowRequest {
        flow: serde_json::from_value(flow)?,
        include_values: true,
        disabled_loops: options.disabled.clone(),
        strategy: options.strategy,
        deliberation: options.deliberation,
        trace: true,
        ..RunFlowRequest::default()
    };
    request.vars.insert("run".to_owned(), run.to_owned());
    if options.memory {
        request.memory = load_memory(Path::new(MEMORY));
    }
    if options.send {
        if mode == "authored" {
            return Err(io::Error::other(
                "--send is not allowed in authored mode: an LLM-written flow's recipient cannot be trusted",
            )
            .into());
        }
        if !recipient_is_exactly_to(&request.flow) {
            return Err(io::Error::other(
                "--send is only allowed for a flow whose recipient slot is exactly ${to}, \
with no other recipient source",
            )
            .into());
        }
        let own = std::env::var("TINYCOMPUTER_LAB_SELF_EMAIL").map_err(|_| {
            io::Error::other("--send needs TINYCOMPUTER_LAB_SELF_EMAIL: it only ever sends to you")
        })?;
        request.vars.insert("to".to_owned(), own);
        request.allow_destructive = true;
    }
    write_json(dir, &format!("request{suffix}.json"), &request)?;
    let result = host.run_flow(&request).await?;
    let exchanges = result
        .trace
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()?;
    std::fs::write(dir.join(format!("jev{suffix}.jsonl")), exchanges.join("\n"))?;
    let mut result = result;
    result.trace.clear();
    write_json(dir, &format!("result{suffix}.json"), &result)?;
    if suffix.is_empty() {
        write_json(dir, "result.json", &result)?;
    }
    if options.memory {
        save_memory(Path::new(MEMORY), &result.learned)?;
    }
    let timeline = flow_timeline(&result);
    std::fs::write(dir.join(format!("timeline{suffix}.txt")), &timeline)?;
    println!("{timeline}");
    Ok(result)
}

/// Whether `flow`'s only recipient source is exactly the `${to}` variable.
///
/// `--send` must never address anywhere but the operator's own inbox, so an
/// authored or hand-edited flow that names a literal address, another
/// variable, or a reply recipient in place of `${to}` is rejected rather than
/// trusted. Only an `enter` slot named `recipient` or `to` whose text is
/// exactly `${to}` counts as the recipient; any other such slot disqualifies
/// the flow.
fn recipient_is_exactly_to(flow: &Flow) -> bool {
    fn walk(steps: &[FlowStep], found: &mut bool, other: &mut bool) {
        for step in steps {
            match step.action() {
                FlowAction::Enter(slots) => {
                    for slot in slots.0 {
                        if slot.slot.eq_ignore_ascii_case("recipient")
                            || slot.slot.eq_ignore_ascii_case("to")
                        {
                            if slot.text.trim() == "${to}" {
                                *found = true;
                            } else {
                                *other = true;
                            }
                        }
                    }
                }
                FlowAction::RepeatUntil(repeat) => walk(&repeat.steps, found, other),
                FlowAction::If(branch) => {
                    walk(&branch.then, found, other);
                    walk(&branch.otherwise, found, other);
                }
                _ => {}
            }
        }
    }

    if !flow.vars.contains_key("to") {
        return false;
    }
    let (mut found, mut other) = (false, false);
    walk(&flow.steps, &mut found, &mut other);
    found && !other
}
