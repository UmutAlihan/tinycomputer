//! Request payloads for the members that act on a ref.
//!
//! Every member here addresses an element allocated by an earlier
//! [`crate::SnapshotRequest`] or [`crate::FindRequest`] and drives it through
//! the platform's accessibility API rather than through synthesized input.
//! That is what makes these members headless by default: pressing a button by
//! its accessibility action does not steal focus, move the cursor, or disturb
//! the pasteboard, so a run does not fight the person using the machine.
//!
//! Most members need nothing but the ref, so most of them share
//! [`RefRequest`]. The rest add exactly the one field their action carries.
//!
//! Before acting, the engine re-resolves the ref and runs an actionability
//! preflight. A ref that no longer resolves fails with `STALE_REF`; an element
//! that resolves but cannot take the action fails with `ACTION_NOT_SUPPORTED`.
//! Neither falls back to clicking at the coordinates the element used to
//! occupy.

mod types;

pub use types::{RefRequest, ScrollRequest, SelectRequest, SetValueRequest, TypeRequest};

#[cfg(test)]
mod test;
