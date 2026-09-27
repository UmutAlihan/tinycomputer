//! The tinycomputer lab: run and score high-level flows on this desktop.
//!
//! ```text
//! scripts/lab list
//! scripts/lab run mail-compose [--mode flow|goal|authored] [--headed] [--send]
//!                              [--disable moves,undo] [--no-memory] [--flow file.json]
//!                              [--strategy narrow|wide]
//! scripts/lab eval all [--modes flow,goal] [--trials 3] [--disable ...]
//! scripts/lab report target/lab-runs/<scenario>/<run>
//! scripts/lab call ListWindows '{"app": "TextEdit"}'   # probe any member
//! scripts/lab guide
//! ```
//!
//! Every run drives real applications and spends Jev (and, in `authored`
//! mode, LLM) credit. Nothing is sent, deleted, or submitted unless `--send`
//! is given, and `--send` only ever addresses `TINYCOMPUTER_LAB_SELF_EMAIL`.

use std::{
    io,
    path::{Path, PathBuf},
    time::Instant,
};

use tinycomputer_bus::{
    FLOW_GUIDE, Flow, FlowAction, FlowLoop, FlowRunResult, FlowStep, RunFlowRequest, RunGoalRequest,
};
use tinycomputer_examples::lab::{
    host::{Host, HostOptions, LabError, module_path},
    record::{
        RunRecord, flow_timeline, goal_timeline, load_memory, report, run_dir, save_memory,
        scorecard, write_json,
    },
    scenario::{SCENARIOS, Scenario, find},
};

const RUNS: &str = "target/lab-runs";
const MEMORY: &str = "target/lab-runs/memory.json";

#[derive(Debug, Default)]
struct Options {
    mode: String,
    modes: Vec<String>,
    trials: u32,
    headed: bool,
    send: bool,
    memory: bool,
    disabled: Vec<FlowLoop>,
    flow_file: Option<PathBuf>,
    strategy: FlowStrategy,
}

#[tokio::main]
async fn main() -> Result<(), LabError> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let command = args.first().map_or("help", String::as_str);
    let target = args.get(1).cloned().unwrap_or_default();
    let options = parse(&args)?;
    match command {
        "list" => {
            for scenario in SCENARIOS {
                println!("{:<22} {}", scenario.name, scenario.app);
            }
            Ok(())
        }
        "guide" => {
            println!("{FLOW_GUIDE}");
            Ok(())
        }
        "report" => {
            println!("{}", report(Path::new(&target))?);
            Ok(())
        }
        "call" => {
            // `lab call <Member> '<json argument>'`: probe any member directly.
            let host = load(&options).await?;
            let argument = args
                .get(2)
                .map(|text| serde_json::from_str(text))
                .transpose()?
                .unwrap_or(serde_json::Value::Null);
            let reply = host.call(&target, argument).await?;
            println!("{}", serde_json::to_string_pretty(&reply)?);
            host.shutdown();
            Ok(())
        }
        "validate" => {
            let host = load(&options).await?;
            let flow = serde_json::from_str(&std::fs::read_to_string(&target)?)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&host.validate(&flow).await?)?
            );
            host.shutdown();
            Ok(())
        }
        "run" => {
            let scenario = find(&target).ok_or_else(|| unknown(&target))?;
            let host = load(&options).await?;
            let record = run_once(&host, scenario, &options.mode, &options).await?;
            host.shutdown();
            println!("{}", scorecard(&[record]));
            Ok(())
        }
        "eval" => {
            let scenarios = if target == "all" {
                SCENARIOS.iter().collect::<Vec<_>>()
            } else {
                target
                    .split(',')
                    .map(|name| find(name).ok_or_else(|| unknown(name)))
                    .collect::<Result<Vec<_>, _>>()?
            };
            let host = load(&options).await?;
            let mut records = Vec::new();
            for scenario in scenarios {
                for mode in &options.modes {
                    for trial in 0..options.trials {
                        println!("\n=== {} / {mode} / trial {}", scenario.name, trial + 1);
                        match run_once(&host, scenario, mode, &options).await {
                            Ok(record) => records.push(record),
                            Err(error) => {
                                println!("run failed: {error}");
                                records.push(failed_run_record(scenario.name, mode, &error));
                            }
                        }
                    }
                }
            }
            host.shutdown();
            let card = scorecard(&records);
            let path = Path::new(RUNS).join(format!(
                "scorecard-{}.md",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_secs())
                    .unwrap_or_default()
            ));
            std::fs::create_dir_all(RUNS)?;
            std::fs::write(&path, &card)?;
            println!("\n{card}\nscorecard written to {}", path.display());
            Ok(())
        }
        _ => {
            println!(
                "usage: lab list | guide | validate <flow.json> | run <scenario> [--mode flow|goal|authored] \
[--headed] [--send] [--disable a,b] [--no-memory] [--flow file] [--strategy narrow|wide] | eval <names|all> [--modes flow,goal] \
[--trials N] | report <run-dir>"
            );
            Ok(())
        }
    }
}

/// A scorecard row for a trial that errored before it produced its own
/// record, so `scorecard` divides by every trial rather than only the ones
/// that ran to completion.
fn failed_run_record(scenario: &str, mode: &str, error: &LabError) -> RunRecord {
    RunRecord {
        scenario: scenario.to_owned(),
        mode: mode.to_owned(),
        passed: false,
        detail: format!("run failed: {error}"),
        stop: "Error".to_owned(),
        actions: 0,
        jev_calls: 0,
        llm_calls: 0,
        seconds: 0.0,
        dir: PathBuf::new(),
    }
}

async fn load(options: &Options) -> Result<Host, LabError> {
    Host::load(
        &module_path(),
        &HostOptions {
            headed: options.headed,
            jev_model: std::env::var("TINYCOMPUTER_LAB_JEV_MODEL").ok(),
        },
    )
    .await
}

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

#[cfg(feature = "inference")]
async fn authored(
    host: &Host,
    scenario: &Scenario,
    options: &Options,
    dir: &Path,
    run: &str,
    llm_calls: &mut u32,
) -> Result<(Option<FlowRunResult>, String, u32, u32), LabError> {
    use tinycomputer_examples::lab::author::{Author, Authored};

    let mut author = Author::from_env(FLOW_GUIDE)?;
    let mut answer = author
        .begin(host, &scenario.brief.replace("{run}", run))
        .await?;
    let (mut last, mut actions, mut jev_calls) = (None, 0, 0);
    for round in 1..=3 {
        let Authored::Flow(flow) = answer else {
            break;
        };
        println!(
            "authored flow, round {round}:\n{}",
            serde_json::to_string_pretty(&flow)?
        );
        write_json(dir, &format!("authored-{round}.json"), &flow)?;
        let result = run_flow(
            host,
            flow,
            options,
            dir,
            &format!("-{round}"),
            run,
            "authored",
        )
        .await?;
        actions += result.actions;
        jev_calls += result.metrics.calls;
        let summary = format!(
            "{}\nvariables read from the screen: {}",
            flow_timeline(&result),
            serde_json::to_string(&result.vars)?
        );
        write_json(dir, "result.json", &result)?;
        last = Some(result);
        answer = author.continue_after(host, &summary).await?;
    }
    *llm_calls = author.calls;
    let stop = last.as_ref().map_or_else(
        || "NoFlow".to_owned(),
        |result| format!("{:?}", result.stop),
    );
    Ok((last, stop, actions, jev_calls))
}

#[cfg(not(feature = "inference"))]
async fn authored(
    _host: &Host,
    _scenario: &Scenario,
    _options: &Options,
    _dir: &Path,
    _run: &str,
    _llm_calls: &mut u32,
) -> Result<(Option<FlowRunResult>, String, u32, u32), LabError> {
    Err(io::Error::other("authored mode needs the `inference` feature").into())
}

fn parse(args: &[String]) -> Result<Options, LabError> {
    let mut options = Options {
        mode: "flow".to_owned(),
        modes: vec!["flow".to_owned(), "goal".to_owned()],
        trials: 1,
        memory: true,
        ..Options::default()
    };
    let skip = if args.first().map(String::as_str) == Some("call") {
        3
    } else {
        2
    };
    let mut rest = args.iter().skip(skip);
    while let Some(flag) = rest.next() {
        let mut value = || {
            rest.next()
                .cloned()
                .ok_or_else(|| io::Error::other(format!("{flag} needs a value")))
        };
        match flag.as_str() {
            "--mode" => options.mode = value()?,
            "--modes" => options.modes = value()?.split(',').map(str::to_owned).collect(),
            "--trials" => options.trials = value()?.parse()?,
            "--headed" => options.headed = true,
            "--send" => options.send = true,
            "--no-memory" => options.memory = false,
            "--flow" => options.flow_file = Some(PathBuf::from(value()?)),
            "--strategy" => {
                options.strategy = serde_json::from_value(serde_json::Value::String(value()?))?;
            }
            "--disable" => {
                options.disabled = value()?
                    .split(',')
                    .map(|name| serde_json::from_value(serde_json::Value::String(name.to_owned())))
                    .collect::<Result<_, _>>()?;
            }
            other => return Err(io::Error::other(format!("unknown flag {other}")).into()),
        }
    }
    Ok(options)
}

/// A short id that makes this run's artifacts on screen distinguishable.
fn run_id() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    format!("{:06}", millis % 1_000_000)
}

fn unknown(name: &str) -> LabError {
    io::Error::other(format!("no scenario named {name:?}; see `lab list`")).into()
}
