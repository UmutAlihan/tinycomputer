//! Reads the engine's Jev debug journal back: lists runs, summarises where a
//! run's wall time went, and prints what Jev was asked and answered.
//!
//! The engine writes one `journal.jsonl` per run when the journal is on
//! (`TINYCOMPUTER_JEV_JOURNAL=1`); `docs/jev-journal.md` describes the events.
//! This module is what the `jev_journal` binary prints with:
//!
//! ```sh
//! cargo run -p tinycomputer-examples --bin jev_journal            # list runs
//! cargo run -p tinycomputer-examples --bin jev_journal -- latest  # summarise
//! ```

#[cfg(test)]
mod test;

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Serialize;
use serde_json::Value;
use tinycomputer_engine::{JOURNAL_DEFAULT_DIR, JOURNAL_ENV, JOURNAL_FILE};

/// The directory runs are journaled under: the value of the journal
/// environment variable when it names a directory, the default otherwise.
#[must_use]
pub fn root() -> PathBuf {
    match std::env::var(JOURNAL_ENV) {
        Ok(value)
            if !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "" | "0" | "1" | "true" | "false" | "on" | "off" | "yes" | "no"
            ) =>
        {
            PathBuf::from(value)
        }
        _ => PathBuf::from(JOURNAL_DEFAULT_DIR),
    }
}

/// The run directories under `root` that hold a journal, oldest first. Run
/// ids start with their start time, so name order is time order; task
/// journals (`task-…`) sort after them.
///
/// # Errors
///
/// Returns the I/O error when `root` cannot be read.
pub fn runs(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut runs = std::fs::read_dir(root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join(JOURNAL_FILE).is_file())
        .collect::<Vec<_>>();
    runs.sort();
    Ok(runs)
}

/// Finds a run under `root` by `name`: `latest`, a whole run id, or the
/// unique run whose id contains `name`. A path to a run directory is used as
/// it is.
///
/// # Errors
///
/// Returns a `NotFound` error when nothing matches, or more than one run does.
pub fn find(root: &Path, name: &str) -> std::io::Result<PathBuf> {
    let direct = PathBuf::from(name);
    if direct.join(JOURNAL_FILE).is_file() {
        return Ok(direct);
    }
    let runs = runs(root)?;
    let matches = if name == "latest" {
        runs.last().cloned().into_iter().collect::<Vec<_>>()
    } else {
        runs.into_iter()
            .filter(|run| {
                run.file_name()
                    .is_some_and(|id| id.to_string_lossy().contains(name))
            })
            .collect()
    };
    match matches.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(not_found(format!("no journaled run matches {name:?}"))),
        many => Err(not_found(format!(
            "{} journaled runs match {name:?}; give more of the id",
            many.len()
        ))),
    }
}

/// Every event in the run at `dir`. A line that does not parse — the last
/// one of a run still being written, say — is skipped.
///
/// # Errors
///
/// Returns the I/O error when the journal cannot be read.
pub fn events(dir: &Path) -> std::io::Result<Vec<Value>> {
    Ok(std::fs::read_to_string(dir.join(JOURNAL_FILE))?
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect())
}

/// Where a run's wall time went, from its events.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct Summary {
    /// The runs journaled in the file, as `kind: label`.
    pub runs: Vec<String>,
    /// Wall time from the journal opening to its last event, in ms.
    pub wall_ms: u64,
    /// Time the loops spent waiting on Jev, in ms: the wall time of each
    /// decision where the flow journals them, else the sum of call latencies.
    pub jev_ms: u64,
    /// Time spent reading the screen, in ms.
    pub observe_ms: u64,
    /// Time spent acting, in ms, not counting settling.
    pub act_ms: u64,
    /// Time spent letting the surface settle after an action, in ms.
    pub settle_ms: u64,
    /// Jev evaluations, counting each framing of a voted decision.
    pub calls: u64,
    /// Evaluations that failed.
    pub failed_calls: u64,
    /// Decisions, each of one or more framings.
    pub decisions: u64,
    /// Screen reads.
    pub observations: u64,
    /// Actions.
    pub actions: u64,
    /// Call latency percentiles, in ms: 50th, 90th, and the maximum.
    pub latency_p50_ms: u64,
    /// See [`Summary::latency_p50_ms`].
    pub latency_p90_ms: u64,
    /// See [`Summary::latency_p50_ms`].
    pub latency_max_ms: u64,
    /// Mean request size, in bytes of JSON.
    pub mean_request_bytes: u64,
    /// Provider-reported input tokens.
    pub input_tokens: u64,
    /// Provider-reported output tokens.
    pub output_tokens: u64,
    /// One row per journaled step, in the order the steps ended.
    pub steps: Vec<StepRow>,
    /// The slowest calls, slowest first.
    pub slowest: Vec<SlowCall>,
}

/// What one flow step spent.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct StepRow {
    /// The step's path, such as `3` or `4.1`.
    pub step: String,
    /// The step kind and text, as the flow report shows it.
    pub text: String,
    /// How it ended.
    pub outcome: String,
    /// Its wall time, in ms; a parent's includes its children's.
    pub wall_ms: u64,
    /// Time its decisions waited on Jev, in ms.
    pub jev_ms: u64,
    /// Time its screen reads took, in ms.
    pub observe_ms: u64,
    /// Time its actions and settling took, in ms.
    pub act_ms: u64,
    /// Jev evaluations it made.
    pub calls: u64,
}

/// One slow Jev call.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct SlowCall {
    /// The event's sequence number, to find it in the file.
    pub seq: u64,
    /// The step it served, if any.
    pub step: String,
    /// Its question ids.
    pub questions: Vec<String>,
    /// Its latency, in ms.
    pub latency_ms: u64,
    /// Its request size, in bytes of JSON.
    pub request_bytes: u64,
}

/// How many slow calls a summary lists.
const SLOWEST: usize = 5;

/// Summarises `events`.
#[must_use]
pub fn summarize(events: &[Value]) -> Summary {
    let mut summary = Summary::default();
    let mut steps: BTreeMap<String, StepRow> = BTreeMap::new();
    let mut order = Vec::new();
    let mut latencies = Vec::new();
    let mut bytes = 0;
    let mut exchange_ms = 0;
    let mut slow = Vec::new();
    for event in events {
        summary.wall_ms = summary.wall_ms.max(number(event, "elapsed_ms"));
        let step = event["step"].as_str().unwrap_or_default().to_owned();
        match event["event"].as_str().unwrap_or_default() {
            "run" => {
                summary
                    .runs
                    .push(format!("{}: {}", text(event, "kind"), text(event, "label")));
            }
            "exchange" => {
                let latency = number(event, "latency_ms");
                summary.calls += 1;
                if event["ok"] != true {
                    summary.failed_calls += 1;
                }
                latencies.push(latency);
                exchange_ms += latency;
                bytes += number(event, "request_bytes");
                summary.input_tokens += number(event, "input_tokens");
                summary.output_tokens += number(event, "output_tokens");
                steps.entry(step.clone()).or_default().calls += 1;
                slow.push(SlowCall {
                    seq: number(event, "seq"),
                    step,
                    questions: strings(&event["questions"]),
                    latency_ms: latency,
                    request_bytes: number(event, "request_bytes"),
                });
            }
            "decision" => {
                let wall = number(event, "wall_ms");
                summary.decisions += 1;
                summary.jev_ms += wall;
                steps.entry(step).or_default().jev_ms += wall;
            }
            "observe" => {
                let wall = number(event, "wall_ms");
                summary.observations += 1;
                summary.observe_ms += wall;
                steps.entry(step).or_default().observe_ms += wall;
            }
            "action" => {
                let (wall, settle) = (number(event, "wall_ms"), number(event, "settle_ms"));
                summary.actions += 1;
                summary.act_ms += wall;
                summary.settle_ms += settle;
                steps.entry(step).or_default().act_ms += wall + settle;
            }
            "step" => {
                let row = steps.entry(step.clone()).or_default();
                row.step.clone_from(&step);
                row.text = format!("{} {}", text(event, "kind"), text(event, "text"));
                row.outcome = text(event, "outcome");
                row.wall_ms = number(event, "wall_ms");
                order.push(step);
            }
            "end" => summary.wall_ms = summary.wall_ms.max(number(event, "wall_ms")),
            _ => {}
        }
    }
    if summary.decisions == 0 {
        // A goal or intent run journals calls, not decisions: each call is
        // one decision the loop waited for in full.
        summary.jev_ms = exchange_ms;
    }
    latencies.sort_unstable();
    summary.latency_p50_ms = percentile(&latencies, 50);
    summary.latency_p90_ms = percentile(&latencies, 90);
    summary.latency_max_ms = latencies.last().copied().unwrap_or_default();
    summary.mean_request_bytes = bytes.checked_div(summary.calls).unwrap_or_default();
    summary.steps = order
        .into_iter()
        .filter_map(|step| steps.remove(&step))
        .collect();
    slow.sort_by(|a, b| b.latency_ms.cmp(&a.latency_ms).then(a.seq.cmp(&b.seq)));
    slow.truncate(SLOWEST);
    summary.slowest = slow;
    summary
}

/// `summary` as text for a terminal.
#[must_use]
pub fn render(summary: &Summary) -> String {
    let mut out = String::new();
    for run in &summary.runs {
        let _ = writeln!(out, "run      {run}");
    }
    let share = |ms: u64| {
        (ms * 100)
            .checked_div(summary.wall_ms)
            .map_or_else(String::new, |percent| format!("{percent:>3}%"))
    };
    let _ = writeln!(out, "wall     {}", seconds(summary.wall_ms));
    let _ = writeln!(
        out,
        "jev      {} {}  {} decisions, {} calls ({} failed); latency p50 {} ms, p90 {} ms, max {} ms; mean request {} B; tokens {} in, {} out",
        seconds(summary.jev_ms),
        share(summary.jev_ms),
        summary.decisions,
        summary.calls,
        summary.failed_calls,
        summary.latency_p50_ms,
        summary.latency_p90_ms,
        summary.latency_max_ms,
        summary.mean_request_bytes,
        summary.input_tokens,
        summary.output_tokens,
    );
    let _ = writeln!(
        out,
        "observe  {} {}  {} reads",
        seconds(summary.observe_ms),
        share(summary.observe_ms),
        summary.observations
    );
    let _ = writeln!(
        out,
        "act      {} {}  {} actions, plus {} settling",
        seconds(summary.act_ms + summary.settle_ms),
        share(summary.act_ms + summary.settle_ms),
        summary.actions,
        seconds(summary.settle_ms).trim(),
    );
    let accounted = summary.jev_ms + summary.observe_ms + summary.act_ms + summary.settle_ms;
    let other = summary.wall_ms.saturating_sub(accounted);
    let _ = writeln!(out, "other    {} {}", seconds(other), share(other));
    if !summary.steps.is_empty() {
        let _ = writeln!(
            out,
            "\n{:<6} {:<9} {:>8} {:>8} {:>8} {:>8} {:>5}  step",
            "path", "outcome", "wall", "jev", "observe", "act", "calls"
        );
        for row in &summary.steps {
            let _ = writeln!(
                out,
                "{:<6} {:<9} {:>8} {:>8} {:>8} {:>8} {:>5}  {}",
                row.step,
                row.outcome,
                seconds(row.wall_ms),
                seconds(row.jev_ms),
                seconds(row.observe_ms),
                seconds(row.act_ms),
                row.calls,
                clip(&row.text, 60)
            );
        }
    }
    if !summary.slowest.is_empty() {
        let _ = writeln!(out, "\nslowest Jev calls");
        for call in &summary.slowest {
            let _ = writeln!(
                out,
                "  #{:<5} {:>6} ms  step {:<5} {:>7} B  {}",
                call.seq,
                call.latency_ms,
                if call.step.is_empty() {
                    "-"
                } else {
                    &call.step
                },
                call.request_bytes,
                call.questions.join(", ")
            );
        }
    }
    out
}

/// Every Jev exchange in `events` as readable lines: the step, the latency,
/// and each question's answer.
#[must_use]
pub fn transcript(events: &[Value]) -> String {
    let mut out = String::new();
    for event in events {
        match event["event"].as_str().unwrap_or_default() {
            "run" => {
                let _ = writeln!(out, "== {}: {}", text(event, "kind"), text(event, "label"));
            }
            "step" => {
                let _ = writeln!(
                    out,
                    "-- step {} {} {:?}: {} ({})",
                    text(event, "step"),
                    text(event, "kind"),
                    text(event, "text"),
                    text(event, "outcome"),
                    text(event, "note"),
                );
            }
            "exchange" => {
                let _ = writeln!(
                    out,
                    "#{} step {} · {} ms · {} B",
                    number(event, "seq"),
                    event["step"].as_str().unwrap_or("-"),
                    number(event, "latency_ms"),
                    number(event, "request_bytes"),
                );
                if event["ok"] != true {
                    let _ = writeln!(out, "    failed: {}", text(event, "error"));
                    continue;
                }
                if let Some(answers) = event["answers"].as_object() {
                    for (id, answer) in answers {
                        let _ = writeln!(out, "    {id:<12} {}", answer_text(answer));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// One answer, compactly: a yes probability, a chosen key, or a top level.
fn answer_text(answer: &Value) -> String {
    let probability = |key: &str| answer["probabilities"][key].as_f64().unwrap_or_default();
    match answer["type"].as_str().unwrap_or_default() {
        "noul" => format!("yes {:.2}", answer["noul"].as_f64().unwrap_or_default()),
        "choice" => {
            let choice = answer["choice"].as_str().unwrap_or_default();
            format!("-> {choice} ({:.2})", probability(choice))
        }
        "score" => format!("score {:.2}", answer["score"].as_f64().unwrap_or_default()),
        _ => answer.to_string(),
    }
}

fn number(event: &Value, key: &str) -> u64 {
    event[key].as_u64().unwrap_or_default()
}

fn text(event: &Value, key: &str) -> String {
    match &event[key] {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The `percent`th percentile of `sorted`, nearest-rank.
fn percentile(sorted: &[u64], percent: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (sorted.len() * percent).div_ceil(100).max(1);
    sorted[rank - 1]
}

/// `ms` as seconds with one decimal, right-aligned.
fn seconds(ms: u64) -> String {
    format!("{:>6.1}s", Duration::from_millis(ms).as_secs_f64())
}

fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut clipped = text.chars().take(limit).collect::<String>();
    clipped.push('…');
    clipped
}

fn not_found(message: String) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::NotFound, message)
}
