//! The engine seam: one command in, one reply out.
//!
//! [`Browser`](crate::Browser) never calls agent-browser directly. It talks to
//! an [`Engine`] per session, opened by a [`Launcher`], so everything above
//! this seam — sessions, conversion, error mapping, outputs — is testable with
//! a scripted engine and no browser.

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

/// The reply to one command, as agent-browser's dispatcher returns it:
/// `{id, success, data}` or `{id, success: false, error}`.
pub type Reply<'a> = Pin<Box<dyn Future<Output = Value> + Send + 'a>>;

/// One browser session's command channel.
pub trait Engine: Send {
    /// Runs one command — an object naming its `action` — and returns the
    /// engine's reply envelope. Commands on one engine never overlap.
    fn execute(&mut self, command: Value) -> Reply<'_>;
}

/// Opens one [`Engine`] per session.
pub trait Launcher: Send + Sync + std::fmt::Debug {
    /// A fresh engine for the session named `session`. Nothing is launched
    /// yet: the session sends an explicit `launch` command first.
    fn open(&self, session: &str) -> Box<dyn Engine>;
}
