//! Interactions: the one member a host calls for everything that changes the
//! page, and the vocabulary for saying which element it means.
//!
//! # Why one member and not fifteen
//!
//! Click, fill, hover, and press differ in what they do to an element, not in
//! how they are addressed, deadlined, or reported. Splitting them into separate
//! bus members would duplicate the target-resolution rules fifteen times over
//! and force a host tool that dispatches on a model-chosen verb to carry its own
//! fifteen-arm match anyway. [`Action`] *is* that match, written once, in the
//! crate both sides share.
//!
//! # Refs, selectors, and locators
//!
//! [`Target`] is deliberately three things. A `@e12` ref comes from a
//! [`crate::Snapshot`] and is what an agent should normally use: it names an
//! element the agent has actually seen, and it fails loudly when the page has
//! moved on. A CSS selector is for a host that already knows the page. A
//! [`Locator`] is for the case an agent is best at — "the button called Submit"
//! — where neither of the other two is expressible.

mod types;

pub use types::{Action, ActionOutcome, LocateBy, Locator, ScrollDirection, Target, WaitState};

#[cfg(test)]
mod test;
