//! Walking a flow's steps in the order they run, checking every text
//! against the variables defined so far and keeping facts out of what Jev
//! may see.

use std::collections::BTreeSet;

use tinycomputer_bus::{FlowAction, FlowStep};

use super::{
    MAX_NESTING, MAX_REPEAT, step_path,
    substitution::{references, substitute_with},
};

/// Walks `steps` in the order they run, checking every text against the
/// variables defined so far and growing that set as `read` steps are seen.
///
/// A variable a `read` step defines only inside an `if` branch or a
/// `repeat_until` body is not carried past it: neither branch of an `if` is
/// guaranteed to run, and `repeat_until` can end after zero rounds when its
/// condition already holds, so a step after either one cannot rely on what
/// only they define.
pub(super) fn walk(
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

/// Every variable a `pick` step stores its item in, at any depth.
pub(super) fn picks(steps: &[FlowStep], picked: &mut BTreeSet<String>) {
    for step in steps {
        match step.action() {
            FlowAction::Pick(pick) => picked.extend(pick.into.clone()),
            FlowAction::RepeatUntil(repeat) => picks(&repeat.steps, picked),
            FlowAction::If(branch) => {
                picks(&branch.then, picked);
                picks(&branch.otherwise, picked);
            }
            _ => {}
        }
    }
}

/// Rejects a condition that names a picked item: the variable holds the
/// whole card's text, clipped, and the opened item seldom shows all of it,
/// so the condition fails on a pick that worked. The pick already fails when
/// nothing fits, so there is nothing left for such a check to prove.
pub(super) fn conditions_on_picks(
    steps: &[FlowStep],
    prefix: &str,
    picked: &BTreeSet<String>,
    errors: &mut Vec<String>,
) {
    for (index, step) in steps.iter().enumerate() {
        let path = step_path(prefix, index);
        let condition = match step.action() {
            FlowAction::Verify(value) | FlowAction::WaitFor(value) => Some(value),
            FlowAction::RepeatUntil(repeat) => {
                conditions_on_picks(&repeat.steps, &path, picked, errors);
                Some(repeat.condition)
            }
            FlowAction::If(branch) => {
                conditions_on_picks(&branch.then, &path, picked, errors);
                conditions_on_picks(&branch.otherwise, &path, picked, errors);
                Some(branch.condition)
            }
            _ => None,
        };
        for name in condition.as_deref().map(references).unwrap_or_default() {
            if picked.contains(&name) {
                errors.push(format!(
                    "step {path}: `${{{name}}}` holds a picked item's whole text, which the screen seldom shows again; a condition must describe what the screen shows, not name a picked item"
                ));
            }
        }
    }
}

/// Rejects every `${name}` in `value` that names a fact: that text is what
/// Jev is asked to reason about, or state it is later shown, and a fact
/// belongs only where an `enter` step types it.
fn forbid_facts(errors: &mut Vec<String>, path: &str, value: &str, facts: &BTreeSet<String>) {
    for name in references(value) {
        if facts.contains(&name) {
            errors.push(format!(
                "step {path}: `${{{name}}}` is a secret; only an enter step may type it — Jev only ever sees it as a name"
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
    bare(errors, path, value, defined);
}

/// Rejects a variable named without `${…}` — `cheapest_flight` rather than
/// `${cheapest_flight}` — where it would be read as the words themselves.
/// Only identifier-shaped names (with `_` or a digit) are looked for, so an
/// ordinary word that happens to name a variable is left alone.
fn bare(errors: &mut Vec<String>, path: &str, value: &str, defined: &BTreeSet<String>) {
    let outside = substitute_with(value, |_| Some(" "));
    let words = outside
        .split(|character: char| !(character.is_alphanumeric() || character == '_'))
        .collect::<BTreeSet<_>>();
    for name in defined {
        let shaped = name.contains('_') || name.chars().any(|character| character.is_ascii_digit());
        if shaped && words.contains(name.as_str()) {
            errors.push(format!(
                "step {path}: `{name}` names a variable; write it as `${{{name}}}` so its value is shown"
            ));
        }
    }
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
