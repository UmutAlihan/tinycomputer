//! The lab: run high-level flows against real applications and score them.
//!
//! - `host` loads the built module through the real `TinyBus` loader.
//! - `scenario` is the task ladder and the checkers that read real state.
//! - `record` writes run artifacts, timelines, and scorecards.
//! - `author` (feature `inference`) is the optional LLM that writes flows.
//!
//! Start with `scripts/lab`; see `docs/lab.md`.

#[cfg(feature = "inference")]
pub mod author;
pub mod host;
pub mod record;
pub mod scenario;
