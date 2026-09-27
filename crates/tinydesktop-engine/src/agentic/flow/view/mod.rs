//! What a flow sees, and the judgements it makes about it.
//!
//! The screen model and its neutral helpers live in `tinydesktop-core`, the
//! desktop's snapshot parsing in `tinydesktop-desktop`; this module re-exports
//! them for the flow runtime and keeps the flow's own policy: when a choice is
//! confident enough to act on, and which controls a flow must not press.

pub(in crate::agentic) use tinydesktop_core::surface::{
    Candidate, Depth, MAX_CANDIDATES, Screen, change_note, describe, exact_named_match,
    fingerprint, label, signature, target_payload, untrusted_context,
};

/// Least probability a target choice needs to be used without re-asking.
pub(in crate::agentic) const ACT: f64 = 0.70;

/// Whether a lower-cased label names an action that is hard to undo.
pub(in crate::agentic) fn destructive_label(evidence: &str) -> bool {
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

/// Whether `label` is named by a `stop_before` phrase the flow itself
/// declares elsewhere.
///
/// A flow that already plans to `stop_before: "sending the email"` has told
/// us, in its own words, that whatever performs that action is irreversible —
/// even when the generic English denylist above does not happen to cover the
/// word it uses. A label under three characters is never checked: it is too
/// short for containment to mean anything ("ok", "go") and would otherwise
/// match almost any phrase.
pub(in crate::agentic) fn named_in_stop_before(label: &str, stop_before: &[String]) -> bool {
    let label = label.trim().to_ascii_lowercase();
    label.chars().count() >= 3
        && stop_before
            .iter()
            .any(|phrase| phrase.to_ascii_lowercase().contains(&label))
}

/// Whether pressing `candidate` on `screen` must be treated as irreversible:
/// its own label names a hard-to-undo action, the flow's own `stop_before`
/// steps already name it, it is an unnamed control offered inside a
/// confirmation sheet — the shape of "Delete"/"Cancel" dialogs whose default
/// button carries no accessible name on some platforms, so the denylist can
/// never see the word that would otherwise gate it — or the screen itself
/// shows payment evidence, so a control worded only "Continue" on a card form
/// is caught even though its own label says nothing about money.
pub(in crate::agentic) fn is_destructive(
    candidate: &Candidate,
    screen: &Screen,
    stop_before: &[String],
) -> bool {
    let name = candidate
        .name
        .as_deref()
        .or(candidate.description.as_deref())
        .unwrap_or_default();
    destructive_label(&label(candidate).to_ascii_lowercase())
        || named_in_stop_before(name, stop_before)
        || (screen.surface == "sheet" && candidate.name.is_none())
        || tinydesktop_core::screen_payment_evidence(screen).is_some()
}

#[cfg(test)]
mod test;
