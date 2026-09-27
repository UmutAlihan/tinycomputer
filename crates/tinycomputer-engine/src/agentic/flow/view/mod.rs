//! What a flow sees, and the judgements it makes about it.
//!
//! The screen model and its neutral helpers live in `tinycomputer-core`, the
//! desktop's snapshot parsing in `tinycomputer-desktop`; this module re-exports
//! them for the flow runtime and keeps the flow's own policy: when a choice is
//! confident enough to act on, and which controls a flow must not press.

pub(in crate::agentic) use tinycomputer_core::surface::{
    Candidate, Depth, Digest, MAX_CANDIDATES, RegionKind, Rendering, Screen, change_note, describe,
    digest, element_line, exact_named_match, fingerprint, label, signature, target_payload,
    untrusted_context,
};

/// Least probability a target choice needs to be used without re-asking.
pub(in crate::agentic) const ACT: f64 = 0.70;

/// Whether a lower-cased label names an action that is hard to undo. A
/// counter's minus button ("remove adult") is not: it only lowers a number.
pub(in crate::agentic) fn destructive_label(evidence: &str) -> bool {
    !tinycomputer_core::adjusts_a_count(evidence)
        && [
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
/// is caught even though its own label says nothing about money. A form
/// control on that page — a card field, an expiry month, a saved-card radio —
/// is not: filling a payment form commits to nothing until its button is
/// pressed, and that button stays gated.
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
        || (!is_form_control(candidate)
            && tinycomputer_core::screen_payment_evidence(screen).is_some())
}

/// Words a purpose is phrased with that say nothing about which element
/// serves it.
const PURPOSE_FILLER: &[&str] = &[
    "the", "and", "for", "with", "into", "from", "that", "this", "click", "press", "expand",
    "scroll", "perform", "accomplish", "step", "choose", "type", "use",
];

/// Reorders `pool` so the elements whose label shares a word stem with
/// `purpose` come first, keeping the order within each group.
///
/// Jev leans toward the first options it is shown: measured on a payment
/// page, "perform: paying for the booking" picked `button "Pay ₹6,840"` at
/// 0.44 when it came first and 0.01 when it came fifth. Putting the
/// elements the purpose names first spends that lean where it helps.
pub(in crate::agentic) fn named_first(purpose: &str, pool: &mut [Candidate]) {
    let wanted = stems(purpose)
        .into_iter()
        .filter(|word| !PURPOSE_FILLER.contains(&word.as_str()))
        .collect::<Vec<_>>();
    if wanted.is_empty() {
        return;
    }
    pool.sort_by_key(|candidate| {
        let named = stems(&label(candidate))
            .iter()
            .any(|word| wanted.iter().any(|want| same_stem(word, want)));
        !named
    });
}

/// The lower-cased words of `text` at least three characters long.
fn stems(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| word.chars().count() >= 3)
        .map(str::to_lowercase)
        .collect()
}

/// Whether two words share a stem: the shorter is a prefix of the longer,
/// or they share their first four characters ("pay" and "paying", "book"
/// and "booking").
fn same_stem(left: &str, right: &str) -> bool {
    let (short, long) = if left.len() <= right.len() {
        (left, right)
    } else {
        (right, left)
    };
    long.starts_with(short)
        || (short.len() >= 4 && long.starts_with(&short[..short.floor_char_boundary(4)]))
}

/// Roles that hold or choose a value rather than submit anything.
const FORM_ROLES: &[&str] = &[
    "textbox",
    "textfield",
    "text field",
    "searchbox",
    "combobox",
    "listbox",
    "option",
    "radio",
    "radiobutton",
    "checkbox",
    "spinbutton",
    "menuitemradio",
    "popupbutton",
];

/// Whether `candidate` holds or chooses a value: a field, a list, an option.
fn is_form_control(candidate: &Candidate) -> bool {
    FORM_ROLES
        .iter()
        .any(|role| candidate.role.eq_ignore_ascii_case(role))
}

#[cfg(test)]
mod test;
