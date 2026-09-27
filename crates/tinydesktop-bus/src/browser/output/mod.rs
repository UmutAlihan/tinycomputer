//! Held outputs: how a screenshot gets from the module to the host.
//!
//! # Why a handle and not the bytes
//!
//! A bus frame is capped at 16 MiB, and a full-page screenshot of a long article
//! at 2x is comfortably larger than that. A served object also cannot open a
//! stream back to its caller — streams ride *alongside* an inbound call — so the
//! module cannot push the image either.
//!
//! What is left is the shape below: the module holds the image, hands back an
//! [`OutputRef`] describing it, and the host pulls it with
//! [`crate::names::methods::READ_OUTPUT`] in chunks it chooses. The `sha256` on
//! the handle is what lets the host verify it reassembled the image the module
//! actually produced rather than a partially-overwritten one.
//!
//! # Release, do not wait for expiry
//!
//! The module bounds what it holds and expires an abandoned output, but until
//! then the slot and the memory are spent. A host that reads an output to
//! completion should release it, and the release belongs in a `defer`-shaped
//! position so a failure part-way through a read does not leak it.

mod types;

pub use types::{ImageFormat, OutputChunk, OutputId, OutputRef, ScreenshotRequest};

#[cfg(test)]
mod test;
