//! The deterministic gates: whether a decision may act, must be confirmed,
//! or abstains, and the local evidence that corroborates or overrides Jev.

use std::collections::BTreeMap;

use tinycomputer_bus::{JevDecisionKind, JevOperation};

use super::super::screen::{Candidate, Screen};
use super::{ACT, CORROBORATED_FLOOR, DESTRUCTIVE, FLOOR};

pub(in crate::agentic) fn gate_with_evidence(
    operation: JevOperation,
    confidence: f64,
    destructive: f64,
    exact_named_match: bool,
) -> JevDecisionKind {
    if operation == JevOperation::Done {
        return if confidence >= ACT {
            JevDecisionKind::Done
        } else {
            JevDecisionKind::Abstain
        };
    }
    if operation == JevOperation::Blocked {
        return if confidence >= ACT {
            JevDecisionKind::Blocked
        } else {
            JevDecisionKind::Abstain
        };
    }
    if confidence < FLOOR && !(exact_named_match && confidence >= CORROBORATED_FLOOR) {
        return JevDecisionKind::Abstain;
    }
    if destructive >= DESTRUCTIVE {
        return JevDecisionKind::ConfirmationRequired;
    }
    if confidence < ACT && !exact_named_match {
        return JevDecisionKind::Abstain;
    }
    JevDecisionKind::Act
}

pub(in crate::agentic) fn exact_named_match(goal: &str, candidate: Option<&Candidate>) -> bool {
    let Some(candidate) = candidate else {
        return false;
    };
    let normalize = |value: &str| {
        let normalized = value
            .chars()
            .map(|character| {
                if character.is_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    ' '
                }
            })
            .collect::<String>();
        normalized.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let goal = normalize(goal);
    let padded_goal = format!(" {goal} ");
    candidate.labels().any(|label| {
        let name = normalize(label);
        let words = name.split_whitespace().collect::<Vec<_>>();
        (2..=words.len()).rev().any(|length| {
            let prefix = words[..length].join(" ");
            padded_goal.contains(&format!(" {prefix} "))
        })
    })
}

pub(in crate::agentic) fn deterministic_destructive(
    goal: &str,
    operation: JevOperation,
    candidate: Option<&Candidate>,
) -> bool {
    if !matches!(operation, JevOperation::Click | JevOperation::TypeText) {
        return false;
    }
    let mut evidence = goal.to_ascii_lowercase();
    if let Some(candidate) = candidate {
        for label in candidate.labels() {
            evidence.push(' ');
            evidence.push_str(&label.to_ascii_lowercase());
        }
    }
    [
        "delete",
        "remove",
        "send",
        "purchase",
        "buy",
        "pay",
        "submit",
        "confirm",
        "overwrite",
        "quit without saving",
        "empty trash",
        "sign out",
    ]
    .iter()
    .any(|term| evidence.contains(term))
}

pub(in crate::agentic) fn positional_match(
    goal: &str,
    candidate: Option<&Candidate>,
    peers: Option<&BTreeMap<String, Candidate>>,
) -> bool {
    let goal = goal.to_ascii_lowercase();
    if !(goal.contains("topmost") || goal.contains("first")) {
        return false;
    }
    let Some(candidate) = candidate else {
        return false;
    };
    let Some(name) = candidate.name.as_deref() else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    if !(name.starts_with("play ") && name.contains(" by ")) {
        return false;
    }
    let Some(y) = candidate
        .bounds
        .as_ref()
        .and_then(|bounds| bounds.get("y"))
        .and_then(serde_json::Value::as_f64)
    else {
        return false;
    };
    peers.is_some_and(|peers| {
        peers
            .values()
            .filter(|peer| {
                peer.name.as_deref().is_some_and(|name| {
                    let name = name.to_ascii_lowercase();
                    name.starts_with("play ") && name.contains(" by ")
                })
            })
            .filter_map(|peer| {
                peer.bounds
                    .as_ref()
                    .and_then(|bounds| bounds.get("y"))
                    .and_then(serde_json::Value::as_f64)
            })
            .all(|peer_y| y <= peer_y)
    })
}

pub(in crate::agentic) fn playing_goal_satisfied(goal: &str, screen: &Screen) -> bool {
    let goal = goal.to_ascii_lowercase();
    if !goal.contains("playing") {
        return false;
    }
    if !(goal.contains("topmost") || goal.contains("first")) {
        return screen.candidates.iter().any(|candidate| {
            candidate
                .name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case("Pause"))
        });
    }
    let mut tracks = screen
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.name.as_deref().is_some_and(|name| {
                let name = name.to_ascii_lowercase();
                name.contains(" by ") && (name.starts_with("play ") || name.starts_with("pause "))
            })
        })
        .filter_map(|candidate| {
            candidate
                .bounds
                .as_ref()
                .and_then(|bounds| bounds.get("y"))
                .and_then(serde_json::Value::as_f64)
                .map(|y| (y, candidate))
        })
        .collect::<Vec<_>>();
    tracks.sort_by(|left, right| left.0.total_cmp(&right.0));
    tracks.first().is_some_and(|(_, candidate)| {
        candidate
            .name
            .as_deref()
            .is_some_and(|name| name.to_ascii_lowercase().starts_with("pause "))
    })
}
