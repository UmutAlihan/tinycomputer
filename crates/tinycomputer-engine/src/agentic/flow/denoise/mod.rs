//! Deterministic denoising: what never needs a question.
//!
//! Every option Jev is shown takes a share of its attention, and a lookalike
//! or an unreachable element takes a share of its probability. Before a
//! grounding asks, this module:
//!
//! - **leaves out** elements that cannot serve any step: disabled ones, and
//!   ones with no area on screen;
//! - **merges** an element nested inside another with the same role and name
//!   — a link wrapping its own label — keeping the outer one;
//! - **ranks by what a person sees**: elements in view first, then those
//!   scrolled out of view (`offscreen`), then those something else covers
//!   (`covered`), which a modal or drawer puts behind itself. Order within a
//!   tier is kept.
//!
//! Across a `do` step's turns it also notices an **oscillation**, the screen
//! returning to where it was two turns ago, and compacts **history** so a
//! repeated line reads once, with a count.
//!
//! The browser's page observer (`sight.js`) denoises at the source — ads,
//! empty containers, hidden elements — before anything reaches here.

use super::view::{Candidate, label};

/// Where an element stands relative to what a person sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Tier {
    /// On screen, in front.
    InView,
    /// Scrolled out of view.
    Offscreen,
    /// Behind something that covers it.
    Covered,
}

/// `candidate`'s tier, from the states the surface reports.
pub(super) fn tier(candidate: &Candidate) -> Tier {
    let has = |state: &str| {
        candidate
            .states
            .iter()
            .any(|held| held.eq_ignore_ascii_case(state))
    };
    if has("covered") {
        Tier::Covered
    } else if has("offscreen") {
        Tier::Offscreen
    } else {
        Tier::InView
    }
}

/// Whether `candidate` can serve no step: it is disabled, or it has bounds
/// with no area.
pub(super) fn inert(candidate: &Candidate) -> bool {
    let disabled = candidate
        .states
        .iter()
        .any(|state| state.eq_ignore_ascii_case("disabled"));
    let empty = candidate.bounds.as_ref().is_some_and(|bounds| {
        let side = |name: &str| bounds.get(name).and_then(serde_json::Value::as_f64);
        matches!(
            (side("width"), side("height")),
            (Some(width), Some(height)) if width <= 0.0 || height <= 0.0
        )
    });
    disabled || empty
}

/// Whether `candidate` sits directly inside an element of `pool` with the
/// same role and name: the inner half of one control exposed twice.
fn nested_twin(candidate: &Candidate, pool: &[Candidate]) -> bool {
    let Some(parent) = candidate.path.last() else {
        return false;
    };
    candidate.name.is_some()
        && *parent == label(candidate)
        && pool.iter().any(|other| {
            other.ref_id != candidate.ref_id
                && label(other) == *parent
                && other.path.len() + 1 == candidate.path.len()
        })
}

/// `pool` denoised: inert elements and nested twins left out, the rest
/// ranked by [`Tier`], order kept within each.
pub(super) fn rank(pool: &[Candidate]) -> Vec<Candidate> {
    let mut kept = pool
        .iter()
        .filter(|candidate| !inert(candidate) && !nested_twin(candidate, pool))
        .cloned()
        .collect::<Vec<_>>();
    kept.sort_by_key(tier);
    kept
}

/// Whether the screen whose fingerprint is `now` is the one of two turns
/// ago, after a different one in between: two actions that undo each other.
pub(super) fn oscillates(seen: &[String], now: &str) -> bool {
    match seen {
        [.., two_ago, last] => two_ago == now && last != now,
        _ => false,
    }
}

/// `history` with each run of identical lines read once, with a count.
pub(super) fn compact(history: &[String]) -> Vec<String> {
    let mut compacted: Vec<(String, usize)> = Vec::new();
    for line in history {
        match compacted.last_mut() {
            Some((last, count)) if last == line => *count += 1,
            _ => compacted.push((line.clone(), 1)),
        }
    }
    compacted
        .into_iter()
        .map(|(line, count)| {
            if count > 1 {
                format!("{line} (x{count})")
            } else {
                line
            }
        })
        .collect()
}

#[cfg(test)]
mod denoise_tests;
