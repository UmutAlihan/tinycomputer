//! The `authored` mode: an LLM writes the flow from the scenario brief (feature
//! `inference`).

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
use super::{Options, run_flow};

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
