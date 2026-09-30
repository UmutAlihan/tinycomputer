//! Session-local denial flag for macOS Apple Events automation.
//!
//! Captures the reactive signal that osascript returns
//! `errAEEventNotPermitted (-1743)` when the calling app lacks an
//! Automation grant for the target. After observation, gated osascript
//! call sites short-circuit until the flag is cleared.
//!
//! Why a reactive flag instead of an in-process probe:
//! `AEDeterminePermissionToAutomateTarget(askUserIfNeeded=false)` would
//! be the principled silent-probe API but it SIGBUSes inside
//! AE.framework's TCC client whenever called from any binary that links
//! the host process (PAC mismatch between arm64 Rust binaries and
//! arm64e Apple frameworks, mediated by `objc2-app-kit` transitive
//! deps). Verified across seven workarounds during #985 plan validation.
//! The osascript stderr `(-1743)` substring is a stable Apple-defined
//! error code that's already produced by the existing fallback path —
//! capturing it costs nothing extra and avoids the FFI entirely.
//!
//! The host can clear the flag after a user changes the Automation grant in
//! System Settings. Clearing only resets the remembered denial; it does not
//! request or grant permission.
//!
//! The host must observe `(-1743)` in an error returned by
//! `focused_text_context` or `focused_text_context_verbose` and call
//! `mark_system_events_denied` itself. This crate does not infer that denial
//! from a query result.

use std::sync::atomic::{AtomicBool, Ordering};

static SYSTEM_EVENTS_DENIED: AtomicBool = AtomicBool::new(false);

/// Mark that osascript has returned -1743 for `tell application "System
/// Events"` in this process.
///
/// Hosts should call this after detecting `(-1743)` in a focus-query error;
/// this crate does not call the function automatically.
pub fn mark_system_events_denied() {
    SYSTEM_EVENTS_DENIED.store(true, Ordering::Relaxed);
}

/// True iff a -1743 has been observed in this process since the last
/// `clear_automation_denial`. The focus fallback checks this and short-circuits
/// before spawning osascript.
pub fn system_events_denied() -> bool {
    SYSTEM_EVENTS_DENIED.load(Ordering::Relaxed)
}

/// Reset the denial flag after a user changes the Automation grant, allowing
/// the next focus query to probe again. The host calls this explicitly.
pub fn clear() {
    SYSTEM_EVENTS_DENIED.store(false, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static M: std::sync::Mutex<()> = std::sync::Mutex::new(());
    M.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
#[path = "automation_state_tests.rs"]
mod tests;
