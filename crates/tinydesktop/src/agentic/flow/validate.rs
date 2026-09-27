//! Flow validation and `${name}` substitution: pure functions, no desktop.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use tinydesktop_bus::{Flow, FlowAction, FlowStep, FlowValidation};

/// Most steps a flow may hold, nested ones included.
const MAX_STEPS: usize = 100;
/// Deepest nesting of `if` and `repeat_until`.
const MAX_NESTING: usize = 4;
/// Most repetitions one `repeat_until` may ask for.
pub(super) const MAX_REPEAT: u32 = 20;

/// Parses and checks a candidate flow.
///
/// `known` are variable names the caller will supply at run time on top of the
/// flow's own `vars`.
pub(super) fn validate(flow: &Value, known: &BTreeSet<String>) -> (Option<Flow>, FlowValidation) {
    let parsed: Flow = match serde_json::from_value(flow.clone()) {
        Ok(parsed) => parsed,
        Err(error) => {
            return (
                None,
                FlowValidation {
                    valid: false,
                    errors: vec![format!("the flow is not well formed: {error}")],
                    steps: 0,
                },
            );
        }
    };
    let validation = check(&parsed, known);
    (validation.valid.then_some(parsed), validation)
}

/// Checks an already-parsed flow.
pub(super) fn check(flow: &Flow, known: &BTreeSet<String>) -> FlowValidation {
    let mut errors = Vec::new();
    if flow.app.trim().is_empty() {
        errors.push("`app` must name the application the flow drives".to_owned());
    }
    if flow.steps.is_empty() {
        errors.push("`steps` must contain at least one step".to_owned());
    }
    let mut defined = known.clone();
    defined.extend(flow.vars.keys().cloned());
    collect_reads(&flow.steps, &mut defined);
    let mut count = 0;
    walk(&flow.steps, "", 0, &defined, &mut count, &mut errors);
    if count > MAX_STEPS {
        errors.push(format!(
            "the flow has {count} steps; at most {MAX_STEPS} are allowed"
        ));
    }
    FlowValidation {
        valid: errors.is_empty(),
        errors,
        steps: count,
    }
}

fn collect_reads(steps: &[FlowStep], defined: &mut BTreeSet<String>) {
    for step in steps {
        match step.action() {
            FlowAction::Read(read) => {
                defined.insert(read.into);
            }
            FlowAction::RepeatUntil(repeat) => collect_reads(&repeat.steps, defined),
            FlowAction::If(branch) => {
                collect_reads(&branch.then, defined);
                collect_reads(&branch.otherwise, defined);
            }
            _ => {}
        }
    }
}

fn walk(
    steps: &[FlowStep],
    prefix: &str,
    depth: usize,
    defined: &BTreeSet<String>,
    count: &mut usize,
    errors: &mut Vec<String>,
) {
    if depth > MAX_NESTING {
        errors.push(format!(
            "step {prefix} nests deeper than {MAX_NESTING} levels"
        ));
        return;
    }
    for (index, step) in steps.iter().enumerate() {
        *count += 1;
        let path = step_path(prefix, index);
        let mut text = |label: &str, value: &str| {
            if value.trim().is_empty() {
                errors.push(format!("step {path}: {label} must not be empty"));
            }
            for name in references(value) {
                if !defined.contains(&name) {
                    errors.push(format!(
                        "step {path}: `${{{name}}}` is not defined in `vars` or by a `read` step"
                    ));
                }
            }
        };
        match step.action() {
            FlowAction::Open(value) => text("the application", &value),
            FlowAction::Do(value) => text("the intent", &value),
            FlowAction::Verify(value) | FlowAction::WaitFor(value) => {
                text("the condition", &value);
            }
            FlowAction::StopBefore(value) => text("the irreversible action", &value),
            FlowAction::Enter(slots) => {
                if slots.0.is_empty() {
                    errors.push(format!("step {path}: `enter` needs at least one slot"));
                }
                for slot in &slots.0 {
                    text("a slot name", &slot.slot);
                    for name in references(&slot.text) {
                        if !defined.contains(&name) {
                            errors.push(format!(
                                "step {path}: `${{{name}}}` is not defined in `vars` or by a `read` step"
                            ));
                        }
                    }
                }
            }
            FlowAction::Choose(choose) => {
                text("`what`", &choose.what);
                text("`option`", &choose.option);
            }
            FlowAction::Read(read) => {
                text("`what`", &read.what);
                if read.into.is_empty()
                    || !read
                        .into
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '_')
                {
                    errors.push(format!(
                        "step {path}: `into` must be a variable name of letters, digits, and `_`"
                    ));
                }
            }
            FlowAction::RepeatUntil(repeat) => {
                text("the condition", &repeat.condition);
                if !(1..=MAX_REPEAT).contains(&repeat.max) {
                    errors.push(format!(
                        "step {path}: `max` must be between 1 and {MAX_REPEAT}"
                    ));
                }
                if repeat.steps.is_empty() {
                    errors.push(format!(
                        "step {path}: `repeat_until` needs at least one step"
                    ));
                }
                walk(&repeat.steps, &path, depth + 1, defined, count, errors);
            }
            FlowAction::If(branch) => {
                text("the condition", &branch.condition);
                if branch.then.is_empty() && branch.otherwise.is_empty() {
                    errors.push(format!(
                        "step {path}: `if` needs a `then` or an `else` branch"
                    ));
                }
                walk(&branch.then, &path, depth + 1, defined, count, errors);
                walk(&branch.otherwise, &path, depth + 1, defined, count, errors);
            }
        }
    }
}

/// `3`, or `4.2` for the second step nested in the fourth.
pub(super) fn step_path(prefix: &str, index: usize) -> String {
    if prefix.is_empty() {
        (index + 1).to_string()
    } else {
        format!("{prefix}.{}", index + 1)
    }
}

/// Every `${name}` referenced in `text`.
pub(super) fn references(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            break;
        };
        names.push(after[..end].to_owned());
        rest = &after[end + 1..];
    }
    names
}

/// Replaces every defined `${name}` in `text`; undefined ones are left as-is.
pub(super) fn substitute(text: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = text.to_owned();
    for (name, value) in vars {
        out = out.replace(&format!("${{{name}}}"), value);
    }
    out
}

/// A stable key for a step or slot: lower-case words, punctuation dropped.
pub(super) fn normalize(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
