//! The answer readers: a Choice's pick, a Noul's probability, a Score's
//! level, and a yes/no calibrated against its negation.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use tinyinference_decisions::{Answer, Choice, EvaluationRequest, Noul, Question, Score};

use crate::agentic::flow::view::{Candidate, Screen, describe, element_line, label, untrusted_context};

use super::{CAP, MAX_READ_SOURCES, MAX_STATE_ELEMENTS, MAX_HISTORY, MAX_FIELDS, request, Questions, numbered, lettered, screen_state::{state, field_contents, ordered_nodes, rich_text}, questions::{condition, negated, reflects, strays, unfinished, completion, progress, coverage, page_kind, helped, asks_for, field_error, obstacle, options, elements, corroborate, only_near, intended, unintended, viewed}};

/// A yes/no probability calibrated against its negation: the mean of
/// `P(yes)` and `1 - P(no)`, or whichever of the two was answered.
pub(in crate::agentic::flow) fn calibrated(answers: &BTreeMap<String, Answer>, yes: &str, no: &str) -> Option<f64> {
    match (probability(answers, yes), probability(answers, no)) {
        (Some(yes), Some(no)) => Some(f64::midpoint(yes, 1.0 - no)),
        (Some(yes), None) => Some(yes),
        (None, Some(no)) => Some(1.0 - no),
        (None, None) => None,
    }
}

/// The probability a Score answer puts on its highest level: "fully
/// accomplished", "all of it holds".
pub(in crate::agentic::flow) fn top_level(answers: &BTreeMap<String, Answer>, id: &str) -> Option<f64> {
    let Some(Answer::Score(answer)) = answers.get(id) else {
        return None;
    };
    let top = answer.probabilities.len().checked_sub(1)?;
    answer.probabilities.get(&top.to_string()).copied()
}

/// Combines a calibrated yes/no with a scale's top-level probability.
pub(in crate::agentic::flow) fn combined(yes_no: Option<f64>, top: Option<f64>) -> Option<f64> {
    match (yes_no, top) {
        (Some(yes_no), Some(top)) => Some(f64::midpoint(yes_no, top)),
        (one, other) => one.or(other),
    }
}

/// The chosen key and its probability, or `None` for `none` or a missing answer.
pub(in crate::agentic::flow) fn chosen(answers: &BTreeMap<String, Answer>, id: &str) -> Option<(String, f64)> {
    match answers.get(id) {
        Some(Answer::Choice(answer)) if answer.choice != "none" => Some((
            answer.choice.clone(),
            answer
                .probabilities
                .get(&answer.choice)
                .copied()
                .unwrap_or_default(),
        )),
        _ => None,
    }
}

/// A Noul's probability, or `None` when it was not asked or not answered.
pub(in crate::agentic::flow) fn probability(answers: &BTreeMap<String, Answer>, id: &str) -> Option<f64> {
    match answers.get(id) {
        Some(Answer::Noul(answer)) => Some(answer.noul),
        _ => None,
    }
}

/// A Score's position as a fraction of the scale, from its level probabilities.
pub(in crate::agentic::flow) fn level(answers: &BTreeMap<String, Answer>, id: &str) -> Option<f64> {
    let Some(Answer::Score(answer)) = answers.get(id) else {
        return None;
    };
    let top = answer.probabilities.len().saturating_sub(1);
    if top == 0 {
        return None;
    }
    let expected = answer
        .probabilities
        .iter()
        .filter_map(|(level, probability)| {
            level
                .parse::<u32>()
                .ok()
                .map(|level| f64::from(level) * probability)
        })
        .sum::<f64>();
    Some(expected / f64::from(u32::try_from(top).unwrap_or(u32::MAX)))
}
