//! What deliberation decided over a run, against how each step ended.

use std::{collections::BTreeMap, fmt::Write as _};

use super::text;
use serde::Serialize;
use serde_json::Value;

/// How one decision site's verdicts fared: how often each verdict was
/// reached there, and how the steps it was reached in ended.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct VerdictRow {
    /// Where it was decided: `target`, `done`, `holds`.
    pub site: String,
    /// `accept`, `deliberate`, or `abstain`.
    pub verdict: String,
    /// Times reached.
    pub count: u32,
    /// Of those, in steps that ended done or already done.
    pub step_done: u32,
    /// Of those, in steps that failed.
    pub step_failed: u32,
    /// Mean winner probability or belief.
    pub mean_p: f64,
    /// Mean share of framings that agreed.
    pub mean_agreement: f64,
}

/// What deliberation did over a run, for tuning its thresholds against how
/// steps actually ended (`docs/technical/specs/jev-deliberation.md`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Calibration {
    /// Every site and verdict reached, in order.
    pub verdicts: Vec<VerdictRow>,
    /// Escalation rungs climbed, by rung.
    pub rungs: BTreeMap<String, u32>,
    /// Duels asked.
    pub duels: u32,
    /// Duels that named a champion.
    pub champions: u32,
    /// Expectations checked, by outcome (`met`, `missed`, `unclear`).
    pub expectations: BTreeMap<String, u32>,
    /// Undos run.
    pub restores: u32,
    /// Undos the screen verified.
    pub restored: u32,
    /// Backtracks tried.
    pub backtracks: u32,
    /// Backtracks whose candidate was confirmed and taken.
    pub branched: u32,
}

/// Tallies what deliberation did in `events` against each step's outcome.
#[must_use]
pub fn calibration(events: &[Value]) -> Calibration {
    let outcomes = events
        .iter()
        .filter(|event| event["event"] == "step")
        .map(|event| (text(event, "step"), text(event, "outcome")))
        .collect::<BTreeMap<_, _>>();
    let mut calibration = Calibration::default();
    let mut rows = BTreeMap::<(String, String), (VerdictRow, f64, f64)>::new();
    for event in events {
        match event["event"].as_str().unwrap_or_default() {
            "evidence" => {
                let key = (text(event, "site"), text(event, "verdict"));
                let (row, p, agreement) = rows.entry(key.clone()).or_insert_with(|| {
                    (
                        VerdictRow {
                            site: key.0.clone(),
                            verdict: key.1.clone(),
                            ..VerdictRow::default()
                        },
                        0.0,
                        0.0,
                    )
                });
                row.count += 1;
                *p += event["p"].as_f64().unwrap_or_default();
                *agreement += event["agreement"].as_f64().unwrap_or_default();
                match outcomes.get(&text(event, "step")).map(String::as_str) {
                    Some("done" | "already_done") => row.step_done += 1,
                    Some("failed") => row.step_failed += 1,
                    _ => {}
                }
            }
            "escalate" => {
                *calibration.rungs.entry(text(event, "rung")).or_default() += 1;
            }
            "duel" => {
                calibration.duels += 1;
                if !event["champion"].is_null() {
                    calibration.champions += 1;
                }
            }
            "expect" => {
                let outcome = text(event, "outcome");
                let kind = outcome.split(':').next().unwrap_or_default().to_owned();
                *calibration.expectations.entry(kind).or_default() += 1;
            }
            "restore" => {
                calibration.restores += 1;
                if event["restored"] == true {
                    calibration.restored += 1;
                }
            }
            "backtrack" => {
                calibration.backtracks += 1;
                if event["accepted"] == true {
                    calibration.branched += 1;
                }
            }
            _ => {}
        }
    }
    calibration.verdicts = rows
        .into_values()
        .map(|(mut row, p, agreement)| {
            let count = f64::from(row.count.max(1));
            row.mean_p = p / count;
            row.mean_agreement = agreement / count;
            row
        })
        .collect();
    calibration
}

/// `calibration` as a table a person reads.
#[must_use]
pub fn render_calibration(calibration: &Calibration) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<10} {:<11} {:>5} {:>6} {:>6} {:>6} {:>7}",
        "site", "verdict", "count", "done", "failed", "mean p", "agreed"
    );
    for row in &calibration.verdicts {
        let _ = writeln!(
            out,
            "{:<10} {:<11} {:>5} {:>6} {:>6} {:>6.2} {:>7.2}",
            row.site,
            row.verdict,
            row.count,
            row.step_done,
            row.step_failed,
            row.mean_p,
            row.mean_agreement
        );
    }
    let rungs = calibration
        .rungs
        .iter()
        .map(|(rung, count)| format!("{rung} {count}"))
        .collect::<Vec<_>>()
        .join(", ");
    let expectations = calibration
        .expectations
        .iter()
        .map(|(outcome, count)| format!("{outcome} {count}"))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(out, "rungs climbed: {rungs}");
    let _ = writeln!(
        out,
        "duels: {} ({} with a champion)",
        calibration.duels, calibration.champions
    );
    let _ = writeln!(out, "expectations: {expectations}");
    let _ = writeln!(
        out,
        "undos: {} ({} verified)  backtracks: {} ({} taken)",
        calibration.restores, calibration.restored, calibration.backtracks, calibration.branched
    );
    out
}
