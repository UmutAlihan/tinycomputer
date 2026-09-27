//! Run artifacts: a directory per run, a readable timeline, and a scorecard.
//!
//! Every run writes `target/lab-runs/<scenario>/<mode>-<timestamp>/` with the
//! request, the raw result, the checker's verdict, and `timeline.txt`: which
//! step did what, which decision loops contributed, and what each action hit.
//! A failed run is diagnosed from its timeline, then fixed, then re-run.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::Value;
use tinydesktop_bus::{FlowRunResult, JevRunResult};

use super::host::LabError;

/// One finished run, as the scorecard counts it.
#[derive(Debug, Clone, Serialize)]
pub struct RunRecord {
    /// Scenario name.
    pub scenario: String,
    /// `flow`, `goal`, or `authored`.
    pub mode: String,
    /// Whether the checker passed.
    pub passed: bool,
    /// What the checker observed.
    pub detail: String,
    /// How the run itself says it stopped.
    pub stop: String,
    /// Desktop actions taken.
    pub actions: u32,
    /// Jev evaluations.
    pub jev_calls: u32,
    /// LLM calls the author made.
    pub llm_calls: u32,
    /// Wall-clock seconds.
    pub seconds: f64,
    /// Where the artifacts are.
    pub dir: PathBuf,
}

/// Creates and returns a fresh run directory.
///
/// # Errors
///
/// Fails when the directory cannot be created.
pub fn run_dir(root: &Path, scenario: &str, mode: &str) -> Result<PathBuf, LabError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    let dir = root.join(scenario).join(format!("{mode}-{stamp}"));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Writes `value` as pretty JSON to `dir/name`.
///
/// # Errors
///
/// Fails when the file cannot be written.
pub fn write_json<T: Serialize>(dir: &Path, name: &str, value: &T) -> Result<(), LabError> {
    fs::write(dir.join(name), serde_json::to_string_pretty(value)?)?;
    Ok(())
}

/// A readable account of a flow run.
#[must_use]
pub fn flow_timeline(result: &FlowRunResult) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "stop: {:?}   actions: {}   jev calls: {}   jev latency: {} ms",
        result.stop, result.actions, result.metrics.calls, result.metrics.latency_ms
    );
    for step in &result.steps {
        let depth = step.path.matches('.').count();
        let indent = "  ".repeat(depth);
        let loops = step
            .loops
            .iter()
            .map(|flow_loop| format!("{flow_loop:?}").to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(",");
        let _ = writeln!(
            out,
            "{indent}[{}] {} {:?} -> {:?} ({} turns, {} jev) [{loops}] {}",
            step.path, step.kind, step.text, step.outcome, step.turns, step.jev_calls, step.note
        );
        for action in &step.actions {
            let target = action
                .target
                .as_ref()
                .map(|target| {
                    format!(
                        " {} {:?}",
                        target.role,
                        target.name.as_deref().unwrap_or_default()
                    )
                })
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "{indent}    - {}{target} ok={} {}",
                action.action, action.ok, action.note
            );
        }
    }
    if let Some(pending) = &result.pending {
        let _ = writeln!(
            out,
            "stopped in front of: {} {:?}",
            pending.role,
            pending.name.as_deref().unwrap_or_default()
        );
    }
    if !result.vars.is_empty() {
        let _ = writeln!(out, "vars: {:?}", result.vars.keys().collect::<Vec<_>>());
    }
    out
}

/// A readable account of a `RunGoal` run.
#[must_use]
pub fn goal_timeline(result: &JevRunResult) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "stop: {:?}   turns: {}   jev calls: {}",
        result.stop,
        result.turns.len(),
        result.metrics.calls
    );
    for turn in &result.turns {
        let target = turn
            .target
            .as_ref()
            .map(|target| {
                format!(
                    " {} {:?}",
                    target.role,
                    target.name.as_deref().unwrap_or_default()
                )
            })
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "[{}] {:?}{target} conf={:.2} ok={} changed={} {}",
            turn.step, turn.operation, turn.confidence, turn.ok, turn.changed, turn.note
        );
    }
    if let Some(pending) = &result.pending {
        let _ = writeln!(out, "pending: {:?} ({})", pending.decision, pending.reason);
    }
    out
}

/// A markdown scorecard over `records`, one row per scenario and mode.
#[must_use]
pub fn scorecard(records: &[RunRecord]) -> String {
    let mut groups: BTreeMap<(String, String), Vec<&RunRecord>> = BTreeMap::new();
    for record in records {
        groups
            .entry((record.scenario.clone(), record.mode.clone()))
            .or_default()
            .push(record);
    }
    let mut out = String::from(
        "| scenario | mode | passed | actions | jev calls | llm calls | seconds |\n|---|---|---|---|---|---|---|\n",
    );
    for ((scenario, mode), runs) in groups {
        let count = f64::from(u32::try_from(runs.len()).unwrap_or(u32::MAX));
        let mean = |field: fn(&RunRecord) -> f64| runs.iter().map(|run| field(run)).sum::<f64>() / count;
        let _ = writeln!(
            out,
            "| {scenario} | {mode} | {}/{} | {:.1} | {:.1} | {:.1} | {:.1} |",
            runs.iter().filter(|run| run.passed).count(),
            runs.len(),
            mean(|run| f64::from(run.actions)),
            mean(|run| f64::from(run.jev_calls)),
            mean(|run| f64::from(run.llm_calls)),
            mean(|run| run.seconds),
        );
    }
    out
}

/// Loads grounding hints saved by earlier runs.
#[must_use]
pub fn load_memory(path: &Path) -> Vec<tinydesktop_bus::GroundingHint> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Merges `learned` into the hints at `path`, newest winning.
///
/// # Errors
///
/// Fails when the file cannot be written.
pub fn save_memory(path: &Path, learned: &[tinydesktop_bus::GroundingHint]) -> Result<(), LabError> {
    let mut hints = load_memory(path);
    for hint in learned {
        hints.retain(|existing| !(existing.app == hint.app && existing.key == hint.key));
        hints.push(hint.clone());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(&hints)?)?;
    Ok(())
}

/// Reads a run directory back into a timeline.
///
/// # Errors
///
/// Fails when the directory holds no readable result.
pub fn report(dir: &Path) -> Result<String, LabError> {
    let text = fs::read_to_string(dir.join("result.json"))?;
    let value: Value = serde_json::from_str(&text)?;
    let mut out = String::new();
    if let Ok(verdict) = fs::read_to_string(dir.join("verdict.json")) {
        let _ = writeln!(out, "verdict: {verdict}");
    }
    if let Ok(flow) = serde_json::from_value::<FlowRunResult>(value.clone()) {
        out.push_str(&flow_timeline(&flow));
    } else if let Ok(goal) = serde_json::from_value::<JevRunResult>(value) {
        out.push_str(&goal_timeline(&goal));
    }
    Ok(out)
}
