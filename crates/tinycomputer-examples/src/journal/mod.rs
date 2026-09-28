//! Reads the engine's Jev debug journal back: lists runs, summarises where a
//! run's wall time went, prints what Jev was asked and answered, and
//! tallies what deliberation decided against how each step ended.
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
mod journal_tests;

use std::time::Duration;

use serde_json::Value;

mod calibration;
mod runs;
mod summary;
mod transcript;

pub use calibration::{Calibration, VerdictRow, calibration, render_calibration};
pub use runs::{events, find, root, runs};
pub use summary::{SlowCall, StepRow, Summary, render, summarize};
pub use transcript::transcript;

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
