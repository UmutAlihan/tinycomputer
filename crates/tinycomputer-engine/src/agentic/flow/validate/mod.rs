//! Flow validation and `${name}` substitution: pure functions, no desktop.

mod substitution;
mod rules;

pub(super) use substitution::{normalize, references, substitute, substitute_safe};

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use tinycomputer_bus::{Flow, FlowAction, FlowStep, FlowValidation};

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
    let facts = carrying_facts(&flow.vars, facts);
    let mut count = 0;
    walk(
        &flow.steps,
        "",
        0,
        &mut defined,
        &facts,
        &mut count,
        &mut errors,
    );
    let mut picked = BTreeSet::new();
    picks(&flow.steps, &mut picked);
    conditions_on_picks(&flow.steps, "", &picked, &mut errors);
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

/// `facts`, plus every flow variable whose own definition names one.
///
/// The runtime expands a flow's `vars` against the caller's values once, so
/// `"first_name": "${first name}"` holds the fact's value from then on and
/// must be kept out of model-facing text exactly like the fact itself.
/// Expansion is a single pass against the caller's values, so one level of
/// definitions is all that can carry a fact.
pub(super) fn carrying_facts(
    flow_vars: &BTreeMap<String, String>,
    facts: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut carrying = facts.clone();
    carrying.extend(
        flow_vars
            .iter()
            .filter(|(_, value)| references(value).iter().any(|name| facts.contains(name)))
            .map(|(name, _)| name.clone()),
    );
    carrying
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
