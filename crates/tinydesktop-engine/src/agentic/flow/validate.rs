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
/// flow's own `vars`. `facts` are the names among them that are the task's
/// facts: a `${name}` for one of those is rejected everywhere except an
/// `enter` step's typed value, so a decision model never sees a fact's value,
/// directly or in the state it is shown on a later step.
pub(super) fn validate(
    flow: &Value,
    known: &BTreeSet<String>,
    facts: &BTreeSet<String>,
) -> (Option<Flow>, FlowValidation) {
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
    let validation = check(&parsed, known, facts);
    (validation.valid.then_some(parsed), validation)
}

/// Checks an already-parsed flow.
pub(crate) fn check(
    flow: &Flow,
    known: &BTreeSet<String>,
    facts: &BTreeSet<String>,
) -> FlowValidation {
    let mut errors = Vec::new();
    if flow.app.trim().is_empty() {
        errors.push("`app` must name the application the flow drives".to_owned());
    }
    if flow.steps.is_empty() {
        errors.push("`steps` must contain at least one step".to_owned());
    }
    let mut defined = known.clone();
    defined.extend(flow.vars.keys().cloned());
    let mut count = 0;
    walk(
        &flow.steps,
        "",
        0,
        &mut defined,
        facts,
        &mut count,
        &mut errors,
    );
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

/// Walks `steps` in the order they run, checking every text against the
/// variables defined so far and growing that set as `read` steps are seen.
///
/// A variable a `read` step defines only inside an `if` branch or a
/// `repeat_until` body is not carried past it: neither branch of an `if` is
/// guaranteed to run, and `repeat_until` can end after zero rounds when its
/// condition already holds, so a step after either one cannot rely on what
/// only they define.
fn walk(
    steps: &[FlowStep],
    prefix: &str,
    depth: usize,
    defined: &mut BTreeSet<String>,
    facts: &BTreeSet<String>,
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
        check_step(&step.action(), &path, depth, defined, facts, count, errors);
    }
}

/// Checks one step's action, recursing into `walk` for `if` and
/// `repeat_until` bodies.
///
/// `model` is every text a Jev evaluation may end up seeing — directly, in a
/// question, or later in `recent_actions` once the step it drove is
/// reported — so a fact there is rejected outright rather than silently
/// expanded at run time. That includes `open` and `browse`: the launched
/// application becomes `screen.app`, and both leave a note in the run's
/// history, so even though the value they act on never reaches Jev as
/// *input*, letting a fact through would still surface it as *state* on
/// every later step. Only an `enter` step's typed value is delivered without
/// ever being echoed back this way, so it alone may carry one.
fn check_step(
    action: &FlowAction,
    path: &str,
    depth: usize,
    defined: &mut BTreeSet<String>,
    facts: &BTreeSet<String>,
    count: &mut usize,
    errors: &mut Vec<String>,
) {
    let model = |errors: &mut Vec<String>, label: &str, value: &str| {
        check_text(errors, path, label, value, defined);
        forbid_facts(errors, path, value, facts);
    };
    match action {
        FlowAction::Open(value) => model(errors, "the application", value),
        FlowAction::Browse(value) => model(errors, "the address", value),
        FlowAction::Do(value) => model(errors, "the intent", value),
        FlowAction::Verify(value) | FlowAction::WaitFor(value) => {
            model(errors, "the condition", value);
        }
        FlowAction::StopBefore(value) => model(errors, "the irreversible action", value),
        FlowAction::Enter(slots) => {
            if slots.0.is_empty() {
                errors.push(format!("step {path}: `enter` needs at least one slot"));
            }
            for slot in &slots.0 {
                // The slot name labels a field for Jev; the text is typed
                // into it locally and never shown, so only the name is
                // checked against `facts`.
                model(errors, "a slot name", &slot.slot);
                undefined(errors, path, &slot.text, defined);
            }
        }
        FlowAction::Choose(choose) => {
            model(errors, "`what`", &choose.what);
            model(errors, "`option`", &choose.option);
        }
        FlowAction::Read(read) | FlowAction::Extract(read) => {
            model(errors, "`what`", &read.what);
            define(errors, path, read.into.clone(), defined);
        }
        FlowAction::Pick(pick) => {
            model(errors, "`from`", &pick.from);
            model(errors, "`by`", &pick.by);
            if let Some(into) = pick.into.clone() {
                define(errors, path, into, defined);
            }
        }
        FlowAction::RepeatUntil(repeat) => {
            model(errors, "the condition", &repeat.condition);
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
            // A round may never run, so what it defines does not survive it.
            let mut inner = defined.clone();
            walk(
                &repeat.steps,
                path,
                depth + 1,
                &mut inner,
                facts,
                count,
                errors,
            );
        }
        FlowAction::If(branch) => {
            model(errors, "the condition", &branch.condition);
            if branch.then.is_empty() && branch.otherwise.is_empty() {
                errors.push(format!(
                    "step {path}: `if` needs a `then` or an `else` branch"
                ));
            }
            // Only one branch runs, so neither one's variables survive it.
            let mut then_defined = defined.clone();
            walk(
                &branch.then,
                path,
                depth + 1,
                &mut then_defined,
                facts,
                count,
                errors,
            );
            let mut else_defined = defined.clone();
            walk(
                &branch.otherwise,
                path,
                depth + 1,
                &mut else_defined,
                facts,
                count,
                errors,
            );
        }
    }
}

/// Rejects every `${name}` in `value` that names a fact: that text is what
/// Jev is asked to reason about, and a fact belongs only where a step types
/// it locally or uses it to reach an address or an application.
fn forbid_facts(errors: &mut Vec<String>, path: &str, value: &str, facts: &BTreeSet<String>) {
    for name in references(value) {
        if facts.contains(&name) {
            errors.push(format!(
                "step {path}: `${{{name}}}` is a fact; use an enter step to type it — Jev only sees slot names"
            ));
        }
    }
}

/// Requires `value` to be non-empty and every `${name}` in it to be defined.
/// Checks that `into` names a variable, and defines it for later steps.
fn define(errors: &mut Vec<String>, path: &str, into: String, defined: &mut BTreeSet<String>) {
    if into.is_empty()
        || !into
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        errors.push(format!(
            "step {path}: `into` must be a variable name of letters, digits, and `_`"
        ));
    } else {
        defined.insert(into);
    }
}

fn check_text(
    errors: &mut Vec<String>,
    path: &str,
    label: &str,
    value: &str,
    defined: &BTreeSet<String>,
) {
    if value.trim().is_empty() {
        errors.push(format!("step {path}: {label} must not be empty"));
    }
    undefined(errors, path, value, defined);
}

fn undefined(errors: &mut Vec<String>, path: &str, value: &str, defined: &BTreeSet<String>) {
    for name in references(value) {
        if !defined.contains(&name) {
            errors.push(format!(
                "step {path}: `${{{name}}}` is not defined in `vars` or by a `read` step"
            ));
        }
    }
}

/// The variables `flow` uses before anything defines them, in first-use
/// order: what a caller must still supply beyond `known`.
pub(crate) fn missing_inputs(
    flow: &Flow,
    known: &BTreeSet<String>,
    facts: &BTreeSet<String>,
) -> Vec<String> {
    let mut missing = Vec::new();
    for error in check(flow, known, facts).errors {
        let Some(start) = error.find("`${") else {
            continue;
        };
        let rest = &error[start + 3..];
        if let Some(end) = rest.find("}` is not defined") {
            let name = rest[..end].to_owned();
            if !missing.contains(&name) {
                missing.push(name);
            }
        }
    }
    missing
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
///
/// Scans `text` once, left to right, and never rescans a value it just
/// substituted in. `read` steps store untrusted screen text in `vars`, so a
/// value that itself looks like `${other}` must stand as literal text rather
/// than expand into `other`'s value.
pub(super) fn substitute(text: &str, vars: &BTreeMap<String, String>) -> String {
    substitute_with(text, |name| vars.get(name).map(String::as_str))
}

/// [`substitute`], but a name in `facts` is treated as undefined and left as
/// literal `${name}` rather than expanded.
///
/// This is the substitution every model-facing text goes through: the
/// runtime's backstop against a fact's value ever reaching Jev, even if
/// validation somehow let a `${fact}` reference through. It never runs on an
/// `enter` step's typed value, or on a `browse` address or `open` application
/// name, which is where a fact is actually delivered.
pub(super) fn substitute_safe(
    text: &str,
    vars: &BTreeMap<String, String>,
    facts: &BTreeSet<String>,
) -> String {
    substitute_with(text, |name| {
        if facts.contains(name) {
            None
        } else {
            vars.get(name).map(String::as_str)
        }
    })
}

fn substitute_with<'a>(text: &'a str, resolve: impl Fn(&str) -> Option<&'a str>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        match resolve(name) {
            Some(value) => out.push_str(value),
            None => out.push_str(&rest[start..=start + 2 + end]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
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
