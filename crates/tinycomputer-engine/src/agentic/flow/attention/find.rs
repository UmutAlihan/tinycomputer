//! Finding distractions without asking anyone: regions in front, regions
//! holding a plain dismiss control, and whatever covers the page.

use std::collections::BTreeSet;

use serde_json::json;
use tinycomputer_bus::{FlowLoop, JevOperation};

use crate::agentic::flow::{
    AgentBackend, FlowRun, Halt, StepLog,
    ask::{self, Questions},
    evidence::{self, Bar, Verdict},
    view::{Candidate, Screen, describe, digest, is_destructive, label, signature},

use super::{MAX_DISTRACTIONS, MAX_CLEARED, ATTENTION_FLOOR, MAX_DISTRACTION_SIZE, Distraction, ESCAPED, Cleared};

/// their rank is their position.
const CLOSERS: &[&[&str]] = &[
    &[
        "reject all",
        "reject",
        "decline",
        "decline all",
        "accept essential only",
        "essential only",
        "necessary only",
        "only necessary",
        "use necessary cookies only",
    ],
    &[
        "close",
        "×",
        "x",
        "✕",
        "dismiss",
        "not now",
        "no thanks",
        "no, thanks",
        "maybe later",
        "skip",
        "later",
        "got it",
        "ok",
        "okay",
        "continue without",
    ],
    &[
        "accept",
        "accept all",
        "agree",
        "i agree",
        "allow all",
        "allow",
    ],
];

/// Words that mark a region as a distraction when it holds no dismiss
/// control of the plainest kind.
const DISTRACTION_WORDS: &[&str] = &[
    "cookie",
    "cookies",
    "consent",
    "privacy",
    "gdpr",
    "newsletter",
    "subscribe",
    "notification",
    "notifications",
    "offer",
    "promo",
    "download",
    "app",
    "survey",
    "feedback",
];

/// The rank of `candidate` as a dismiss control, lower is less committal;
/// `None` when it is not one.
fn closer_rank(candidate: &Candidate) -> Option<usize> {
    let name = candidate
        .name
        .as_deref()
        .or(candidate.description.as_deref())?
        .trim()
        .to_lowercase();
    let clickable = candidate
        .available_actions
        .iter()
        .any(|action| action == "Click");
    if !clickable {
        return None;
    }
    CLOSERS
        .iter()
        .position(|rank| rank.contains(&name.as_str()))
}

/// `candidate`'s own accessible name or description, lower-cased and
/// trimmed — the word [`distractions`] tells an unambiguous dismissal from a
/// generic "OK"/"Okay" by.
fn closer_label(candidate: &Candidate) -> String {
    candidate
        .name
        .as_deref()
        .or(candidate.description.as_deref())
        .unwrap_or_default()
        .trim()
        .to_lowercase()
}

fn words(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The distractions on `screen` a step about `intent` may need cleared first,
/// at most [`MAX_DISTRACTIONS`], those in front first. `cleared` holds the
/// signatures of controls already pressed this step, which are not offered
/// again.
///
/// A distraction is the container a dismiss control sits in, with every
/// element under it: the digest's regions are too coarse on a small page,
/// where a toast and the form beside it share one.
pub(in crate::agentic::flow) fn distractions(
    screen: &Screen,
    intent: &str,
    stop_before: &[String],
    cleared: &BTreeSet<String>,
) -> Vec<Distraction> {
    let intent = words(intent);
    let named_by_step = |text: &str| {
        words(text).iter().any(|word| {
            word.len() > 3 && DISTRACTION_WORDS.contains(&word.as_str()) && intent.contains(word)
        })
    };
    let in_front = digest(screen)
        .front()
        .flat_map(|region| region.members.iter().copied())
        .collect::<BTreeSet<_>>();
    // Each container's least committal dismiss control, in page order.
    let mut containers: Vec<(Vec<String>, usize, &Candidate)> = Vec::new();
    for candidate in &screen.candidates {
        let Some(rank) = closer_rank(candidate) else {
            continue;
        };
        if is_destructive(candidate, screen, stop_before) || cleared.contains(&signature(candidate))
        {
            continue;
        }
        match containers
            .iter_mut()
            .find(|(path, _, _)| *path == candidate.path)
        {
            Some(entry) if rank < entry.1 => *entry = (candidate.path.clone(), rank, candidate),
            Some(_) => {}
            None => containers.push((candidate.path.clone(), rank, candidate)),
        }
    }
    let mut found = Vec::new();
    for (path, rank, closer) in containers {
        let members = screen
            .candidates
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.path.starts_with(&path))
            .collect::<Vec<_>>();
        // A form is the step's, whatever its close icon says: a
        // newsletter prompt holds one field, a passenger form several.
        let fields = members
            .iter()
            .filter(|(_, member)| {
                member
                    .available_actions
                    .iter()
                    .any(|action| action == "SetValue" || action == "TypeText")
            })
            .count();
        if members.len() > MAX_DISTRACTION_SIZE || fields > 1 {
            continue;
        }
        let front = members.iter().any(|(index, _)| in_front.contains(index));
        let text = path
            .iter()
            .cloned()
            .chain(members.iter().map(|(_, member)| label(member)))
            .collect::<Vec<_>>()
            .join(" ");
        let named = words(&text)
            .iter()
            .any(|word| DISTRACTION_WORDS.contains(&word.as_str()));
        let marked = front || named;
        // A plain "Close" says enough; an "Accept" or "Reject" in ordinary
        // content, with nothing to say it is a distraction, is the step's.
        // "OK"/"Okay" say nothing either way — the same word confirms a
        // deletion as readily as it dismisses a toast — so, unlike the rest
        // of this tier, they need the container's own words to say it is
        // boilerplate; being merely frontmost is not enough, or a
        // destructive confirmation dialog would be clicked through as a
        // distraction before its `stop_before` is ever reached.
        let accepted = if matches!(closer_label(closer).as_str(), "ok" | "okay") {
            named
        } else {
            marked || rank == 1
        };
        if !accepted || named_by_step(&text) {
            continue;
        }
        found.push((
            !front,
            Distraction {
                name: path
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "top level".to_owned()),
                shows: members
                    .iter()
                    .take(6)
                    .map(|(_, member)| label(member))
                    .collect(),
                closer: Some(closer.clone()),
            },
        ));
    }
    found.sort_by_key(|(behind, _)| *behind);
    let mut found = found
        .into_iter()
        .take(MAX_DISTRACTIONS)
        .map(|(_, distraction)| distraction)
        .collect::<Vec<_>>();
    if found.len() < MAX_DISTRACTIONS
        && let Some(covering) = covering(screen, &intent, cleared)
    {
        found.push(covering);
    }
    found
}

/// Something open over the page with no control of its own — a calendar
/// or list left open by an earlier step — when the surface marks elements
/// `covered`: a distraction Escape clears. Live on Emirates, the date
/// calendar stayed open over the form and covered the Class button the next
/// step needed.
pub(super) fn covering(screen: &Screen, intent: &[String], cleared: &BTreeSet<String>) -> Option<Distraction> {
    if cleared.contains(ESCAPED) {
        return None;
    }
    let covered = screen
        .candidates
        .iter()
        .filter(|candidate| {
            candidate
                .states
                .iter()
                .any(|state| state.eq_ignore_ascii_case("covered"))
        })
        .collect::<Vec<_>>();
    if covered.is_empty() {
        return None;
    }
    // A step about the thing in front — "choose the date in the calendar" —
    // works in it; only a step about what lies under it is covered.
    let front = screen
        .candidates
        .iter()
        .filter(|candidate| {
            !covered
                .iter()
                .any(|hidden| hidden.ref_id == candidate.ref_id)
        })
        .map(label)
        .collect::<Vec<_>>();
    let needed = covered
        .iter()
        .filter(|candidate| {
            words(&label(candidate))
                .iter()
                .any(|word| word.len() > 3 && intent.contains(word))
        })
        .map(|candidate| format!("{} (covered: the step needs it)", label(candidate)))
        .collect::<Vec<_>>();
    if needed.is_empty() {
        return None;
    }
    // What the step needs and cannot reach comes first: it is the reason to
    // clear, and what lies over it only says what it is.
    Some(Distraction {
        name: "something open over the page".to_owned(),
        shows: needed.into_iter().chain(front).take(6).collect(),
        closer: None,
    })
}

/// A distraction as a Choice option Jev reads, wrapped as untrusted data.
pub(in crate::agentic::flow) fn option(distraction: &Distraction, include_values: bool) -> serde_json::Value {
    let cleared_with = distraction.closer.as_ref().map_or_else(
        || serde_json::json!("press Escape"),
        |closer| describe(closer, include_values),
    );
    serde_json::json!({"untrusted_accessibility_data": {
        "region": distraction.name,
        "shows": distraction.shows,
        "cleared_with": cleared_with,
    }})
}
