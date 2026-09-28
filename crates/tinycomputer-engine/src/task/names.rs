//! The names a task's flow may use: the facts', the flow's own, the secret
//! ones, and a guess at the kind of value each one is.

use std::collections::BTreeSet;

use tinycomputer_bus::Flow;
use tinycomputer_bus::agent::InputKind;
use tinycomputer_core::Facts;

/// A best guess at a value's kind from its name, for the caller's form.
pub(crate) fn input_kind(name: &str) -> InputKind {
    let name = name.to_ascii_lowercase();
    if name.contains("email") {
        InputKind::Email
    } else if name.contains("phone") || name.contains("mobile") {
        InputKind::Phone
    } else if name.contains("date") || name.contains("birth") || name.contains("dob") {
        InputKind::Date
    } else if name.contains("count") || name.contains("number of") || name.contains("travellers") {
        InputKind::Number
    } else {
        InputKind::Text
    }
}

pub(super) fn known_names(flow: &Flow, facts: &Facts) -> BTreeSet<String> {
    facts
        .names()
        .into_iter()
        .map(str::to_owned)
        .chain(flow.vars.keys().cloned())
        .collect()
}

/// The secret names among `facts`: what the flow validator and runtime
/// treat as never allowed in model-facing text.
pub(super) fn fact_names(facts: &Facts) -> BTreeSet<String> {
    facts
        .secret_names()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

pub(super) fn is_undefined(error: &str) -> bool {
    error.contains("` is not defined in `vars`")
}
