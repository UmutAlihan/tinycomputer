//! Expectations: what an action should change, checked against what it did.
//!
//! A completion judge sees only the screen after the fact, and a press can
//! succeed and still do the wrong thing: a checkbox that was already ticked
//! is cleared, a link opens another page where a dropdown was wanted. Before
//! a `do` move presses an element, its [`Effect`] is predicted from the move
//! and the element's role and state alone; after it, the screens before and
//! after are compared. A clear contradiction is a [`Outcome::Missed`], a
//! suspected mistake: the next judgement asks Jev whether the action did what
//! it was meant to, and a mistake Jev confirms is undone (`checkpoint/`).
//!
//! Only contradictions count. An effect that cannot be confirmed either way
//! is [`Outcome::Unclear`] and changes nothing, so a page that opens a menu
//! in its own way is never mistaken for a wrong click.

use super::view::{Candidate, Screen, difference, label};

/// Roles whose press selects them: one of a set.
const SELECTS: &[&str] = &[
    "radio",
    "radiobutton",
    "tab",
    "option",
    "menuitemradio",
    "listitem",
];

/// Roles whose press flips them.
const FLIPS: &[&str] = &["checkbox", "switch", "menuitemcheckbox", "togglebutton"];

/// Roles whose press opens something.
const OPENS: &[&str] = &["combobox", "popupbutton", "menubutton"];

/// Words of a control that closes what it sits on.
const CLOSERS: &[&str] = &[
    "close",
    "cancel",
    "dismiss",
    "done",
    "ok",
    "got it",
    "not now",
    "no thanks",
];

/// What a press should change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Effect {
    /// Something opens: a menu, a dropdown, a disclosure.
    Opens,
    /// The overlay the control sits on closes.
    Closes,
    /// Another page or view loads.
    Navigates,
    /// The element ends up selected (`true`) or not (`false`).
    Toggles(bool),
    /// The view scrolls.
    Scrolls,
    /// Nothing can be predicted.
    Unknown,
}

impl Effect {
    /// What the press was meant to do, as Jev reads it.
    pub(super) fn meant(&self, target: &str) -> String {
        match self {
            Self::Opens => format!("open the menu, list, or section of {target}"),
            Self::Closes => format!("close what {target} sits on"),
            Self::Navigates => format!("go to what {target} leads to"),
            Self::Toggles(true) => format!("select {target}"),
            Self::Toggles(false) => format!("clear {target}"),
            Self::Scrolls => format!("scroll {target}"),
            Self::Unknown => format!("advance the step by pressing {target}"),
        }
    }

    /// Whether undoing it means pressing the element again.
    pub(super) fn toggles(&self) -> bool {
        matches!(self, Self::Toggles(_))
    }
}

/// What the screens before and after an action show about its [`Effect`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Outcome {
    /// The effect is plainly there.
    Met,
    /// The screen contradicts it; says how.
    Missed(String),
    /// Neither confirmed nor contradicted.
    Unclear,
}

fn role_is(candidate: &Candidate, roles: &[&str]) -> bool {
    roles
        .iter()
        .any(|role| candidate.role.eq_ignore_ascii_case(role))
}

fn has_state(candidate: &Candidate, state: &str) -> bool {
    candidate
        .states
        .iter()
        .any(|held| held.eq_ignore_ascii_case(state))
}

/// Whether `candidate` shows selected: checked, selected, or pressed.
pub(super) fn selected(candidate: &Candidate) -> bool {
    ["checked", "selected", "pressed", "on"]
        .iter()
        .any(|state| has_state(candidate, state))
}

/// What `operation` on `target` should change.
pub(super) fn predict(operation: &str, target: &Candidate) -> Effect {
    match operation {
        "scroll" => return Effect::Scrolls,
        "expand" => return Effect::Opens,
        _ => {}
    }
    if role_is(target, SELECTS) {
        return Effect::Toggles(true);
    }
    if role_is(target, FLIPS) {
        return Effect::Toggles(!selected(target));
    }
    if role_is(target, OPENS) || has_state(target, "collapsed") {
        return Effect::Opens;
    }
    if role_is(target, &["link"]) {
        return Effect::Navigates;
    }
    let name = target
        .name
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if CLOSERS.contains(&name.as_str()) {
        return Effect::Closes;
    }
    Effect::Unknown
}

/// `target` as it stands on `screen`: the element with its role, name, and
/// place.
pub(super) fn find<'s>(target: &Candidate, screen: &'s Screen) -> Option<&'s Candidate> {
    screen.candidates.iter().find(|candidate| {
        candidate.role == target.role
            && candidate.name == target.name
            && candidate.path == target.path
    })
}

/// What `before` and `after` show about `effect` of pressing `target`;
/// `moved` says the surface's address changed in between.
pub(super) fn check(
    effect: &Effect,
    target: &Candidate,
    before: &Screen,
    after: &Screen,
    moved: bool,
) -> Outcome {
    let (appeared, _) = difference(before, after);
    match effect {
        Effect::Toggles(wanted) => {
            if moved {
                return Outcome::Missed(format!(
                    "pressing {} left the page instead of changing it",
                    label(target)
                ));
            }
            match find(target, after) {
                Some(now) if selected(now) == *wanted => Outcome::Met,
                Some(_) if *wanted => Outcome::Missed(format!(
                    "{} does not show selected after it was pressed",
                    label(target)
                )),
                Some(_) => Outcome::Missed(format!(
                    "{} still shows selected after it was pressed to clear it",
                    label(target)
                )),
                None => Outcome::Unclear,
            }
        }
        Effect::Opens if moved => Outcome::Missed(format!(
            "pressing {} left the page instead of opening something",
            label(target)
        )),
        Effect::Opens if !appeared.is_empty() || before.surface != after.surface => Outcome::Met,
        Effect::Navigates if moved || before.window != after.window => Outcome::Met,
        Effect::Closes if before.surface != "window" && after.surface == "window" => Outcome::Met,
        _ => Outcome::Unclear,
    }
}

#[cfg(test)]
mod expect_tests;
