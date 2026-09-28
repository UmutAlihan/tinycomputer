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

mod clear;
mod find;

use std::collections::BTreeSet;

use super::view::Candidate;

/// Most distractions one attention question offers.
pub(super) const MAX_DISTRACTIONS: usize = 4;
/// Distractions cleared per step at most.
pub(super) const MAX_CLEARED: u32 = 3;
/// Least probability a distraction must win the attention Choice with.
pub(super) const ATTENTION_FLOOR: f64 = 0.5;
/// Most elements a distraction holds. A toast, a consent card, or a prompt
/// is small; a container holding more is the page, and its "close" icon
/// clears a field or a panel the step may need (live on Emirates, the
/// booking form's clear icons sat directly under `main`).
pub(super) const MAX_DISTRACTION_SIZE: usize = 12;

/// Something on screen that may need clearing before the step.
#[derive(Debug, Clone)]
pub(super) struct Distraction {
    /// Where it sits: the digest's name for its region.
    pub(super) name: String,
    /// A few of its labels, as Jev reads them.
    pub(super) shows: Vec<String>,
    /// The control that clears it, least committal of those it holds; `None`
    /// for something that covers the page with no control of its own, which
    /// Escape clears.
    pub(super) closer: Option<Candidate>,
}

/// The key a step's Escape at something covering the page is remembered
/// under, so a covering Escape did not close is not offered again.
pub(super) const ESCAPED: &str = "escape: whatever covers the page";

/// What a step has cleared so far.
#[derive(Debug, Default)]
pub(super) struct Cleared {
    /// Signatures of the controls pressed.
    pub(super) pressed: BTreeSet<String>,
    /// How many.
    pub(super) count: u32,
}

#[cfg(test)]
mod attention_tests;
