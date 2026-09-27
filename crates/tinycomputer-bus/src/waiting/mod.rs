//! The request payload for the member that blocks until something happens.
//!
//! Polling a snapshot in a loop is the obvious way to wait and the wrong one:
//! it burns a full tree walk per attempt and races the redraw it is waiting
//! for. [`WaitRequest`] pushes the wait into the engine, which watches the
//! accessibility notifications the platform already emits.
//!
//! One mode field is set per call. They are separate fields rather than a
//! tagged enum because that is the shape the engine's own argument struct has,
//! and reshaping it here would put a translation nobody checks between two
//! definitions of the same thing.

mod types;

pub use types::WaitRequest;

#[cfg(test)]
mod test;
