//! Every Jev exchange in a run as readable lines.

use std::fmt::Write as _;

use super::{number, text};
use serde_json::Value;

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
