//! Where a run's wall time went: the summary and its rendering for a terminal.

use std::{collections::BTreeMap, fmt::Write as _};

use super::{clip, number, percentile, seconds, strings, text};
use serde::Serialize;
use serde_json::Value;

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
    /// The largest single call's input tokens.
    pub max_input_tokens: u64,
    /// The largest call's share of Jev's context window, in percent.
    pub max_window_percent: u64,
    /// `do` turns journaled.
    pub turns: u64,
    /// Decisions a `do` turn waited for, one after another: the most, and
    /// the mean in hundredths.
    pub max_turn_decisions: u64,
    /// See [`Summary::max_turn_decisions`].
    pub mean_turn_decisions_x100: u64,
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
/// Jev's context window, in tokens.
const JEV_WINDOW: u64 = 32_000;

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
    let mut turn_decisions = 0;
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
                summary.max_input_tokens =
                    summary.max_input_tokens.max(number(event, "input_tokens"));
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
            "turn" => {
                // Decisions asked in one batch share a round trip; a turn
                // waits for its rounds, not its decisions.
                let decisions = event
                    .get("rounds")
                    .map_or_else(|| number(event, "decisions"), |_| number(event, "rounds"));
                summary.turns += 1;
                turn_decisions += decisions;
                summary.max_turn_decisions = summary.max_turn_decisions.max(decisions);
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
    summary.max_window_percent = summary.max_input_tokens * 100 / JEV_WINDOW;
    summary.mean_turn_decisions_x100 = (turn_decisions * 100)
        .checked_div(summary.turns)
        .unwrap_or_default();
    summary.steps = order
        .into_iter()
        .filter_map(|step| steps.remove(&step))
        .collect();
    slow.sort_by(|a, b| b.latency_ms.cmp(&a.latency_ms).then(a.seq.cmp(&b.seq)));
    slow.truncate(SLOWEST);
    summary.slowest = slow;
    summary
}

/// The summary's lines about the shape of the Jev calls: how much of the
/// window the largest used, and how many decisions a turn waited for.
fn shape(summary: &Summary, out: &mut String) {
    if summary.max_input_tokens > 0 {
        let _ = writeln!(
            *out,
            "window   largest call {} tokens, {}% of Jev's {} K",
            summary.max_input_tokens,
            summary.max_window_percent,
            JEV_WINDOW / 1000
        );
    }
    if summary.turns > 0 {
        let _ = writeln!(
            *out,
            "turns    {} do turns; decisions in sequence per turn: mean {}.{:02}, most {}",
            summary.turns,
            summary.mean_turn_decisions_x100 / 100,
            summary.mean_turn_decisions_x100 % 100,
            summary.max_turn_decisions
        );
    }
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
    shape(summary, &mut out);
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
