//! Browser automation over the [agent-browser] engine.
//!
//! This crate is the browser counterpart of `tinydesktop-desktop`: an adapter
//! that turns the typed requests in [`tinydesktop_bus::browser`] into engine
//! commands and the engine's replies into typed results. It holds no bus, no
//! agent loop, and no model; `tinydesktop-engine` drives it, and `tinydesktop`
//! serves it over `TinyBus` as `ai.tinyhumans.tinydesktop.Browser`.
//!
//! [agent-browser]: https://github.com/vercel-labs/agent-browser
//!
//! # What is here
//!
//! - [`Browser`] — sessions and every call on them: navigate, snapshot,
//!   perform, read, evaluate, screenshot, downloads, and held outputs.
//! - [`Engine`] and [`Launcher`] — the seam to agent-browser: one JSON command
//!   in, one reply out, one engine per session. Tests script it; the linked
//!   engine implements it over agent-browser's dispatcher.
//! - [`Error`] — what can go wrong, as a taxonomy of what a caller should do
//!   next, each variant mapped to one published wire name.
//!
//! Every browser contract type is re-exported, so `tinydesktop_browser::Action`
//! is the same type as `tinydesktop_bus::browser::Action`.

mod convert;
mod engine;
mod error;
mod outputs;
mod reply;
mod session;

pub use engine::{Engine, Launcher, Reply};
pub use error::{Error, Result};
pub use session::{Browser, MAX_SESSIONS};
pub use tinydesktop_bus::browser::*;
