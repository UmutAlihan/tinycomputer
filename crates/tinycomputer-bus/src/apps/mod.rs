//! Request payloads for the members that manage applications and windows.
//!
//! These members sit outside any one application's accessibility tree: they
//! start and stop processes, enumerate what is running, and move windows
//! around. They are what a caller uses to reach a known state before
//! [`crate::observation`] takes over.
//!
//! An application is named the way a person names it — `Safari`, `Slack` —
//! and the engine resolves the name against running processes, bundle
//! identifiers, and installed applications. A name matching more than one
//! candidate fails with `AMBIGUOUS_TARGET` and lists them, rather than picking
//! one.

mod types;

pub use types::{
    CloseAppRequest, FocusWindowRequest, LaunchRequest, ListAppsRequest, ListSurfacesRequest,
    ListWindowsRequest, MoveWindowRequest, ResizeWindowRequest, WindowRequest,
};

#[cfg(test)]
mod test;
