//! The `authored` mode: an LLM writes the flow from the scenario brief (feature
//! `inference`).

use std::path::Path;

use super::Options;
use crate::run::run_flow;
use tinycomputer_bus::{FLOW_GUIDE, FlowRunResult};
use tinycomputer_examples::lab::{
    host::{Host, LabError},
    record::{flow_timeline, write_json},
    scenario::Scenario,
};

#[cfg(feature = "inference")]
pub(crate) async fn authored(
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
pub(crate) async fn authored(
    _host: &Host,
    _scenario: &Scenario,
    _options: &Options,
    _dir: &Path,
    _run: &str,
    _llm_calls: &mut u32,
) -> Result<(Option<FlowRunResult>, String, u32, u32), LabError> {
    Err(io::Error::other("authored mode needs the `inference` feature").into())
}
