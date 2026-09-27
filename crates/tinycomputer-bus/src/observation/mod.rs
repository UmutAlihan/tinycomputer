//! Request payloads for the members that read the screen without touching it.
//!
//! Observation is where a session starts. [`SnapshotRequest`] walks an
//! application's accessibility tree and allocates a *ref* for every element it
//! returns — a compact, qualified handle like `@s8f3k2p9:e1` that every
//! interaction member takes in place of coordinates. [`FindRequest`] filters
//! the same walk down to the elements matching a query. [`GetRequest`] and
//! [`IsRequest`] read one property of one already-allocated ref, and
//! [`ScreenshotRequest`] falls back to pixels for the cases a tree cannot
//! describe.
//!
//! Refs go stale when the application redraws. A member that is handed a stale
//! ref fails with `STALE_REF` and a recovery hint asking for a fresh snapshot;
//! it does not silently act on whatever now occupies that position.

mod types;

pub use types::{FindRequest, GetRequest, IsRequest, ScreenshotRequest, SnapshotRequest};

#[cfg(test)]
mod test;
