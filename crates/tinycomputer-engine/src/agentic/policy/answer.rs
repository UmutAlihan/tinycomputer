//! Reading Jev's answers: a choice and its probability, a destructiveness
//! score, an operation name, and the target it picked.

use tinycomputer_bus::JevOperation;
use tinyinference_decisions::Answer;

use super::super::screen::Candidate;
use super::request::ActionSpace;

pub(in crate::agentic) fn choice(answer: Option<&Answer>) -> Option<(&str, f64)> {
    match answer {
        Some(Answer::Choice(answer)) => Some((
            &answer.choice,
            answer
                .probabilities
                .get(&answer.choice)
                .copied()
                .unwrap_or_default(),
        )),
        _ => None,
    }
}

pub(in crate::agentic) fn noul(answer: Option<&Answer>) -> f64 {
    match answer {
        Some(Answer::Noul(answer)) => answer.noul,
        _ => 1.0,
    }
}

pub(in crate::agentic) fn parse_operation(value: &str) -> Option<JevOperation> {
    Some(match value {
        "CLICK" => JevOperation::Click,
        "TYPE_TEXT" => JevOperation::TypeText,
        "CHECK" => JevOperation::Check,
        "UNCHECK" => JevOperation::Uncheck,
        "EXPAND" => JevOperation::Expand,
        "COLLAPSE" => JevOperation::Collapse,
        "SCROLL" => JevOperation::Scroll,
        "DRILL" => JevOperation::Drill,
        "WIDEN" => JevOperation::Widen,
        "WAIT" => JevOperation::Wait,
        "DONE" => JevOperation::Done,
        "BLOCKED" => JevOperation::Blocked,
        _ => return None,
    })
}

pub(in crate::agentic) fn target<'a>(
    space: &'a ActionSpace,
    operation_name: &str,
    answer: Option<&Answer>,
) -> Option<(&'a Candidate, f64)> {
    let (choice, confidence) = choice(answer)?;
    if choice == "none" {
        return None;
    }
    space
        .targets
        .get(operation_name)?
        .get(choice)
        .map(|node| (node, confidence))
}
