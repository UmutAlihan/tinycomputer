//! Attention: the root of every turn's decision tree.
//!
//! Before a step is judged or an element grounded, the runtime asks what on
//! the screen needs attention first: the step itself, or a distraction — a
//! cookie or privacy card, a promo toast lying over the results, a
//! newsletter prompt. A distraction left in place takes Jev's attention,
//! covers the element a step needs, and turns a click into a miss.
//!
//! The candidates are found without asking anyone ([`distractions`]): the
//! regions the screen digest puts in front (dialogs, consent and newsletter
//! regions), and any region holding a plain dismiss control (×, Close, Not
//! now, Reject all). A region the step itself names is the step's business,
//! not a distraction, and a control that looks irreversible is never offered.
//! Only when there is a candidate is Jev asked, with one Choice, and only a
//! clearly agreed pick (`evidence.rs`) is cleared, with the region's
//! least-committal control: rejecting or essential-only first, closing next,
//! accepting last.

use super::view::{
    Candidate, RegionKind, Screen, describe, digest, is_destructive, label, signature,
};

/// Most distractions one attention question offers.
pub(super) const MAX_DISTRACTIONS: usize = 4;
/// Distractions cleared per step at most.
pub(super) const MAX_CLEARED: u32 = 3;
/// Least probability a distraction must win the attention Choice with.
pub(super) const ATTENTION_FLOOR: f64 = 0.5;

/// Labels of controls that dismiss what they sit on, least committal first:
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
        "close", "×", "x", "✕", "dismiss", "not now", "no thanks", "no, thanks", "maybe later",
        "skip", "later", "got it", "ok", "okay", "continue without",
    ],
    &["accept", "accept all", "agree", "i agree", "allow all", "allow"],
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

/// Something on screen that may need clearing before the step.
#[derive(Debug, Clone)]
pub(super) struct Distraction {
    /// Where it sits: the digest's name for its region.
    pub(super) name: String,
    /// A few of its labels, as Jev reads them.
    pub(super) shows: Vec<String>,
    /// The control that clears it, least committal of those it holds.
    pub(super) closer: Candidate,
}

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

fn words(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The distractions on `screen` a step about `intent` may need cleared first,
/// at most [`MAX_DISTRACTIONS`], front regions first. `cleared` holds the
/// signatures of controls already pressed this step, which are not offered
/// again.
pub(super) fn distractions(
    screen: &Screen,
    intent: &str,
    stop_before: &[String],
    cleared: &std::collections::BTreeSet<String>,
) -> Vec<Distraction> {
    let intent = words(intent);
    let named_by_step = |text: &str| {
        words(text)
            .iter()
            .any(|word| word.len() > 3 && DISTRACTION_WORDS.contains(&word.as_str()) && intent.contains(word))
    };
    let mut found = Vec::new();
    let mut regions = digest(screen).regions;
    regions.sort_by_key(|region| region.kind != RegionKind::Front);
    for region in regions {
        let members = region
            .members
            .iter()
            .filter_map(|index| screen.candidates.get(*index))
            .collect::<Vec<_>>();
        let closer = members
            .iter()
            .filter(|candidate| !is_destructive(candidate, screen, stop_before))
            .filter(|candidate| !cleared.contains(&signature(candidate)))
            .filter_map(|candidate| Some((closer_rank(candidate)?, *candidate)))
            .min_by_key(|(rank, _)| *rank);
        let Some((rank, closer)) = closer else {
            continue;
        };
        let text = std::iter::once(region.name.clone())
            .chain(members.iter().map(|member| label(member)))
            .collect::<Vec<_>>()
            .join(" ");
        let marked = region.kind == RegionKind::Front
            || words(&text)
                .iter()
                .any(|word| DISTRACTION_WORDS.contains(&word.as_str()));
        // A plain "Close" in ordinary content, with nothing to say it is a
        // distraction, is as likely a panel the step needs.
        if !marked && rank != 1 {
            continue;
        }
        if named_by_step(&text) {
            continue;
        }
        found.push(Distraction {
            name: region.name,
            shows: members.iter().take(6).map(|member| label(member)).collect(),
            closer: closer.clone(),
        });
        if found.len() >= MAX_DISTRACTIONS {
            break;
        }
    }
    found
}

/// A distraction as a Choice option Jev reads, wrapped as untrusted data.
pub(super) fn option(distraction: &Distraction, include_values: bool) -> serde_json::Value {
    serde_json::json!({"untrusted_accessibility_data": {
        "region": distraction.name,
        "shows": distraction.shows,
        "cleared_with": describe(&distraction.closer, include_values),
    }})
}

#[cfg(test)]
mod test;
