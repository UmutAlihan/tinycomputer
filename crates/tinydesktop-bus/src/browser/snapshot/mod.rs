//! The accessibility snapshot: what an agent looks at before it acts.
//!
//! A snapshot is the page's accessibility tree rendered as indented text, with a
//! `@e12` ref on every element an agent could plausibly act on. It is what an
//! agent should read instead of HTML: it is an order of magnitude smaller, it
//! already excludes what a screen reader would not announce, and every line it
//! contains is addressable by [`crate::Target::Ref`].
//!
//! The refs belong to the snapshot that produced them. [`Snapshot::sequence`]
//! records which one that was, so a host holding an old snapshot can tell that
//! the page has moved on without waiting for an action to fail.

mod types;

pub use types::{ElementRef, Snapshot, SnapshotRequest};

#[cfg(test)]
mod test;
