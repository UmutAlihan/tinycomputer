//! Picking one element for a purpose, with as few and as small questions as
//! the screen allows.
//!
//! 1. **Memory**: an element that grounded the same step before is offered
//!    first and confirmed with one yes/no question.
//! 2. **Narrowing**: a pool larger than [`CAP`] is split by region (the
//!    ancestor it sits under). One round trip asks which region holds the
//!    element and a knockout of `CAP`-sized groups cut along the regions;
//!    the chosen region's winners go on to the Choice.
//! 3. **Choice** over at most `CAP` elements.
//! 4. **Consistency and corroboration**: a low-confidence pick is re-asked
//!    with relabelled options, and confirmed with a yes/no question, in one
//!    request. It is used only if the evidence agrees.
//!
//! A deliberating run (`docs/specs/jev-deliberation.md`) changes three
//! things. The pool is denoised first (`denoise.rs`): what is in view ranks
//! ahead of what is not. Narrowing keeps the two best regions wherever the
//! region answer is close — an early wrong branch is the one grounding can
//! never recover from — and at the deep level, when the region answer left
//! group winners out, the final Choice is asked a second time over every
//! winner, to cross-check the region against a pick that never used it. And
//! step 4 gives way to the evidence gate and its escalation ladder
//! (`escalate`).
//!
//! The first round is built by [`FlowRun::opening`] without being asked, so
//! a `do` turn can send it with its judge, and finished by
//! [`FlowRun::resume`].

mod decide;
mod narrow;

use std::collections::BTreeMap;

use tinyinference_decisions::{Answer, EvaluationRequest};

use super::{
    ask::{CAP, chosen},
    view::Candidate,
};

/// Least probability an exact-name match needs to be used without re-asking.
pub(super) const NAMED_FLOOR: f64 = 0.45;
/// A corroboration this confident accepts a target on its own.
pub(super) const CORROBORATED: f64 = 0.8;
/// A corroboration this confident accepts a target the re-ask agreed on.
pub(super) const AGREED: f64 = 0.5;
/// Deepest ancestor level narrowing splits on.
const MAX_REGION_DEPTH: usize = 8;
/// Lead the chosen region needs over the next one for narrowing to follow
/// it alone; closer, a deliberating run keeps both.
pub(super) const BRANCH_MARGIN: f64 = 0.3;

/// Named groups of candidates, largest first.
pub(super) type Regions = Vec<(String, Vec<Candidate>)>;

/// An element chosen for a purpose.
#[derive(Debug, Clone)]
pub(super) struct Grounded {
    pub(super) candidate: Candidate,
    pub(super) confidence: f64,
}

/// The first round grounding asks, built but not yet sent, so a caller can
/// batch it with a request of its own: the turn's judge asks it alongside,
/// and uses the answers only when the move turns out to need a target.
#[derive(Debug, Clone)]
pub(super) struct Opening {
    purpose: String,
    pool: Vec<Candidate>,
    first: First,
}

/// What an [`Opening`] asks.
#[derive(Debug, Clone)]
enum First {
    /// Nothing to ask: no pool, or a remembered element used unconfirmed.
    Settled(Option<Grounded>),
    /// A remembered element, confirmed with one yes/no question.
    Remembered {
        known: Candidate,
        request: EvaluationRequest,
    },
    /// A crowded pool: a knockout whose groups follow the screen's regions,
    /// and, when the pool splits, which region holds the element — asked
    /// together, in one round trip.
    Narrowed {
        groups: Vec<(Option<usize>, Vec<Candidate>)>,
        regions: Option<(Vec<String>, Regions)>,
        requests: Vec<EvaluationRequest>,
    },
    /// A pool small enough for one Choice.
    Chosen {
        keys: Vec<String>,
        request: EvaluationRequest,
    },
}

impl Opening {
    /// The requests to send, the one grounding needs most first.
    pub(super) fn requests(&self) -> Vec<EvaluationRequest> {
        match &self.first {
            First::Settled(_) => Vec::new(),
            First::Remembered { request, .. } | First::Chosen { request, .. } => {
                vec![request.clone()]
            }
            First::Narrowed { requests, .. } => requests.clone(),
        }
    }
}

/// The knockout's group winners, in page order. When the region question
/// chose a region holding at least one winner, only the winners of the
/// `kept` regions go on: its answer is the coarse look a person takes
/// first, and a close second region is kept beside it.
fn winners(
    knockout: &BTreeMap<String, Answer>,
    groups: Vec<(Option<usize>, Vec<Candidate>)>,
    kept: &[usize],
    regions: Option<&(Vec<String>, Regions)>,
) -> Vec<Candidate> {
    let won = groups
        .into_iter()
        .enumerate()
        .filter_map(|(index, (home, group))| {
            let (choice, _) = chosen(knockout, &format!("group_{index}"))?;
            let position = choice.parse::<usize>().ok()?.checked_sub(1)?;
            group.into_iter().nth(position).map(|winner| (home, winner))
        })
        .collect::<Vec<_>>();
    if kept.is_empty() {
        return won.into_iter().map(|(_, winner)| winner).collect();
    }
    let inside = |home: Option<usize>, winner: &Candidate| {
        home.map_or_else(
            || {
                kept.iter().any(|region| {
                    regions
                        .and_then(|(_, regions)| regions.get(*region))
                        .is_some_and(|(_, members)| {
                            members.iter().any(|member| member.ref_id == winner.ref_id)
                        })
                })
            },
            |home| kept.contains(&home),
        )
    };
    if won.iter().any(|(home, winner)| inside(*home, winner)) {
        won.into_iter()
            .filter(|(home, winner)| inside(*home, winner))
            .map(|(_, winner)| winner)
            .collect()
    } else {
        won.into_iter().map(|(_, winner)| winner).collect()
    }
}

/// Groups `pool` by the first ancestor level, at or below `from`, that splits
/// it into more than one region. Regions beyond `CAP - 1` are merged.
pub(super) fn split(pool: &[Candidate], from: usize) -> Option<(usize, Regions)> {
    for level in from..MAX_REGION_DEPTH {
        let mut regions: BTreeMap<String, Vec<Candidate>> = BTreeMap::new();
        for candidate in pool {
            let region = candidate
                .path
                .get(level)
                .cloned()
                .unwrap_or_else(|| "top level".to_owned());
            regions.entry(region).or_default().push(candidate.clone());
        }
        if regions.len() < 2 {
            continue;
        }
        let mut regions = regions.into_iter().collect::<Vec<_>>();
        regions.sort_by_key(|(_, members)| std::cmp::Reverse(members.len()));
        if regions.len() > CAP {
            let rest = regions
                .split_off(CAP - 1)
                .into_iter()
                .flat_map(|(_, members)| members)
                .collect::<Vec<_>>();
            regions.push(("everything else".to_owned(), rest));
        }
        return Some((level, regions));
    }
    None
}
