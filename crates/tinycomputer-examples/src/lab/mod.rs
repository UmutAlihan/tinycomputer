//! The lab: run high-level flows against real applications and score them.
//!
//! - `host` is [`crate::host`], re-exported: the built module loaded through
//!   the real `TinyBus` loader.
//! - `scenario` is the task ladder and the checkers that read real state.
//! - `record` writes run artifacts, timelines, and scorecards.
//! - `author` (feature `inference`) is the optional LLM that writes flows.
//!
//! Start with `scripts/lab`; see `docs/technical/lab.md`.

#[cfg(feature = "inference")]
pub mod author;
pub use crate::host;
pub mod record;
pub mod scenario;
