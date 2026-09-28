//! Matching an option to the controls on screen: which mention it, which
//! already show it chosen, and which to press without judgement.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use tinycomputer_bus::{
    ChooseStep, FlowAction, FlowLoop, FlowStopReason, IfStep, JevOperation, PickStep, ReadStep,
    RepeatStep, StepOutcome,
};
use tinycomputer_core::surface::{Group, result_families};
use tinycomputer_core::{Criterion, Record, rank};

use crate::workspace::BROWSER;

use crate::agentic::flow::{
    Ended, FlowRun, Halt, StepLog,
    act::{DONE, SCREEN_VIEW},
    ask::{self, Questions, chosen, condition, numbered},
    backend::{AgentBackend, deliver_text},
    escalate::Belief,
    ground::Grounded,
    memory::{learn, remember},
    validate::{MAX_REPEAT, substitute_safe},
    view::{Candidate, Screen, element_kind, is_destructive, label, target_payload},
};

use super::*;

/// Every text field on `screen` that holds text, with that text: what a
/// failed `choose` puts back.
pub(super) fn held_text(screen: &Screen) -> Vec<(Candidate, String)> {
    screen
        .candidates
        .iter()
        .filter(|candidate| {
            candidate
                .available_actions
                .iter()
                .any(|action| action == "SetValue" || action == "TypeText")
        })
        .filter_map(|candidate| {
            let text = candidate.value.as_ref()?.as_str()?.trim();
            (!text.is_empty()).then(|| (candidate.clone(), text.to_owned()))
        })
        .collect()
}

/// The matches whose labels say little besides the option: a container
/// whose label strings together everything inside it (a calendar button
/// named with every day of the month) is dropped when a plainer match exists.
pub(in crate::agentic::flow) fn closest(matches: Vec<Candidate>) -> Vec<Candidate> {
    let length = |candidate: &Candidate| candidate.name.as_deref().map_or(0, str::len);
    let Some(shortest) = matches.iter().map(length).min() else {
        return matches;
    };
    matches
        .into_iter()
        .filter(|candidate| length(candidate) <= shortest.saturating_mul(3).max(shortest + 40))
        .collect()
}

/// Lower-case words joined by single spaces, so `Sunday, 18 October` and
/// `sunday 18 october` compare equal.
pub(super) fn plain(text: &str) -> String {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Words beyond the option's own that a label may carry and still be the
/// option ("Srinagar, SXR Srinagar International Airport").
const OPTION_EXTRA_WORDS: usize = 12;

/// Roles of a control that is one option however much its label says: a
/// fare card's radio names its price, baggage, and rules, and is still just
/// "Saver".
const ONE_OPTION_ROLES: &[&str] = &[
    "radio",
    "radiobutton",
    "option",
    "menuitemradio",
    "checkbox",
];

/// Whether a control is checked or selected already.
pub(super) fn is_checked(candidate: &Candidate) -> bool {
    candidate
        .states
        .iter()
        .any(|state| state == "checked" || state == "selected")
}

pub(super) fn is_one_option(candidate: &Candidate) -> bool {
    ONE_OPTION_ROLES
        .iter()
        .any(|role| candidate.role.eq_ignore_ascii_case(role))
}

/// The option control on `screen` that is already checked or selected and
/// whose label starts with `option`: there is nothing to choose.
pub(in crate::agentic::flow) fn already_chosen(screen: &Screen, option: &str) -> Option<Candidate> {
    let wanted = plain(option);
    if wanted.is_empty() {
        return None;
    }
    screen
        .candidates
        .iter()
        .find(|candidate| {
            is_one_option(candidate)
                && is_checked(candidate)
                && candidate
                    .name
                    .as_deref()
                    .is_some_and(|name| plain(name).starts_with(&wanted))
        })
        .cloned()
}

/// The field on `screen` that already shows exactly `option` as its value,
/// when the flow did not type it there (`typed`, by `element_kind`): a
/// passengers box reading "1 Adult" has that choice made, and pressing the
/// stepper beside it, whose label also mentions "1 Adult", would change it.
pub(in crate::agentic::flow) fn already_holds(
    screen: &Screen,
    option: &str,
    typed: &BTreeSet<String>,
) -> Option<Candidate> {
    let wanted = plain(option);
    if wanted.is_empty() {
        return None;
    }
    screen
        .candidates
        .iter()
        .find(|candidate| {
            candidate
                .value
                .as_ref()
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| plain(value) == wanted)
                && !typed.contains(&element_kind(candidate))
        })
        .cloned()
}

/// Roles a page marks as the one chosen among its siblings.
const SELECTABLE_ROLES: &[&str] = &["tab", "radio", "radiobutton", "option", "menuitemradio"];

/// Why `screen` plainly shows `option` not chosen: the tab or radio named
/// exactly `option` is not selected while a sibling of its kind is; or, when
/// the step typed to filter a list (`filtered`), the option it pressed is
/// still offered there unselected — a press that took closes the list or
/// marks the option. `None` when nothing on screen settles it, and Jev is
/// asked instead.
pub(in crate::agentic::flow) fn left_unchosen(screen: &Screen, option: &str, filtered: bool) -> Option<String> {
    let wanted = plain(option);
    if wanted.is_empty() {
        return None;
    }
    let selectable = |candidate: &&Candidate| {
        SELECTABLE_ROLES
            .iter()
            .any(|role| candidate.role.eq_ignore_ascii_case(role))
    };
    let asked = screen
        .candidates
        .iter()
        .filter(selectable)
        .find(|candidate| {
            candidate
                .name
                .as_deref()
                .is_some_and(|name| plain(name) == wanted)
        })?;
    if is_checked(asked) {
        return None;
    }
    if filtered && asked.role.eq_ignore_ascii_case("option") {
        return Some(format!("{} is still offered, unselected", label(asked)));
    }
    let other = screen
        .candidates
        .iter()
        .filter(selectable)
        .find(|candidate| {
            candidate.role == asked.role
                && container(&candidate.path) == container(&asked.path)
                && is_checked(candidate)
        })?;
    Some(format!(
        "{} is not selected; {} is",
        label(asked),
        label(other)
    ))
}

/// The ancestors siblings share: `path` without the numbered items it ends
/// in, since each tab of a strip sits in its own `listitem #n`.
pub(super) fn container(path: &[String]) -> &[String] {
    let numbered = |segment: &&String| {
        segment
            .rsplit_once(" #")
            .is_some_and(|(_, number)| number.parse::<u32>().is_ok())
    };
    let kept = path.len() - path.iter().rev().take_while(numbered).count();
    &path[..kept]
}

/// Whether a label says far more than the option: a control whose name
/// strings together a whole list (recent searches, every day of a month)
/// mentions the option without being it. An option control is never such a
/// list, however long its label.
pub(in crate::agentic::flow) fn lists_more_than(candidate: &Candidate, option: &str) -> bool {
    if is_one_option(candidate) {
        return false;
    }
    let words = |text: &str| {
        plain(text)
            .split(' ')
            .filter(|word| !word.is_empty())
            .count()
    };
    candidate
        .name
        .as_deref()
        .is_some_and(|name| words(name) > words(option) + OPTION_EXTRA_WORDS)
}

/// Whether an element takes typed text.
pub(super) fn editable(candidate: &Candidate) -> bool {
    candidate
        .available_actions
        .iter()
        .any(|action| action == "SetValue")
}

/// Whether every match carries the same label, as a day's button and its
/// grid cell do.
pub(super) fn one_option(matches: &[Candidate]) -> bool {
    let mut labels = matches
        .iter()
        .map(|candidate| plain(candidate.name.as_deref().unwrap_or_default()));
    labels
        .next()
        .is_some_and(|first| labels.all(|label| label == first))
}

/// The match to press without judgement: a button, option, or link before a
/// cell or container, then the shortest label.
pub(super) fn plainest(matches: Vec<Candidate>) -> Option<Candidate> {
    let rank = |candidate: &Candidate| {
        let role = match candidate.role.as_str() {
            "button" | "option" | "menuitem" | "link" | "radio" => 0,
            _ => 1,
        };
        (role, candidate.name.as_deref().map_or(0, str::len))
    };
    matches.into_iter().min_by_key(rank)
}

/// Whether an element shows `option` in its name, value, or description. A
/// date matches by its day, month, and year, whatever the weekday or order
/// (`Sunday, 18 October 2026` shows `18 October 2026`).
pub(super) fn mentions(candidate: &Candidate, option: &str) -> bool {
    let wanted = plain(option);
    let date = looks_like_date(option).then(|| date_words(option));
    !wanted.is_empty()
        && [
            candidate.name.clone(),
            candidate.description.clone(),
            candidate.value.as_ref().map(ToString::to_string),
        ]
        .into_iter()
        .flatten()
        .any(|text| {
            let shown = format!(" {} ", plain(&text));
            match &date {
                Some(words) => words
                    .iter()
                    .all(|word| shown.contains(&format!(" {word} "))),
                // "Srinagar (SXR)" is the "Srinagar ... Airport SXR" row: the
                // exact phrase, or else every one of its words.
                None => {
                    shown.contains(&format!(" {wanted} "))
                        || wanted
                            .split(' ')
                            .all(|word| shown.contains(&format!(" {word} ")))
                }
            }
        })
}

/// What to type to find `option` in a search box: its name before any
/// qualifier, so "Srinagar (SXR)" searches for "Srinagar" — a box matching
/// on the name would find nothing for the whole of it.
pub(in crate::agentic::flow) fn search_text(option: &str) -> String {
    let name = option
        .split(['(', ','])
        .next()
        .map(str::trim)
        .unwrap_or_default();
    if name.is_empty() {
        option.trim().to_owned()
    } else {
        name.to_owned()
    }
}

/// The matches in `pool` inside the region `what` names, or all of them
/// when the page places none there.
///
/// `what` names the control the option belongs to (a seat picker, a
/// destination search); a page rarely echoes that description on the
/// option's own label, so an unrelated control elsewhere that happens to
/// share the option's text must not qualify. Some pages carry no region
/// text at all, and narrowing then would drop every real option.
pub(super) fn within(pool: Vec<Candidate>, what: &str) -> Vec<Candidate> {
    let regional = pool
        .iter()
        .filter(|candidate| in_region(candidate, what))
        .cloned()
        .collect::<Vec<_>>();
    if regional.is_empty() { pool } else { regional }
}

/// Whether `candidate` sits inside — or itself names — the region `what`
/// describes. A page rarely echoes a description such as "the outbound
/// flight list" on an option's own label, so this also checks the option's
/// ancestor labels (`path`), which the snapshot records outermost first.
pub(in crate::agentic::flow) fn in_region(candidate: &Candidate, what: &str) -> bool {
    let wanted = plain(what);
    if wanted.is_empty() {
        return true;
    }
    let names = |text: &str| format!(" {} ", plain(text)).contains(&format!(" {wanted} "));
    [candidate.name.as_deref(), candidate.description.as_deref()]
        .into_iter()
        .flatten()
        .any(names)
        || candidate.path.iter().any(|ancestor| names(ancestor))
}

/// A copy of `candidate` with its shown text stripped, for logging a private
/// choice without writing the value it displayed into history.
pub(in crate::agentic::flow) fn redacted(candidate: &Candidate) -> Candidate {
    Candidate {
        name: None,
        description: None,
        value: None,
        ..candidate.clone()
    }
}

/// Elements that can be pressed.
pub(super) fn clickable(candidates: &[Candidate]) -> Vec<Candidate> {
    candidates
        .iter()
        .filter(|candidate| {
            candidate
                .available_actions
                .iter()
                .any(|action| action == "Click")
        })
        .cloned()
        .collect()
}
