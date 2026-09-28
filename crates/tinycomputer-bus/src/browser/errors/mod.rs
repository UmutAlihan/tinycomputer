//! The error names the module answers with, and what a host should do about
//! each one.
//!
//! # Why the name matters more than the message
//!
//! A host does not show a model a raw failure string; it decides what *kind* of
//! failure happened and shapes the tool result accordingly. A bad selector is
//! something a model can fix by taking a fresh snapshot. A browser that will not
//! launch is not. A navigation blocked by policy must never be retried. Those
//! are three different tool results, and telling them apart by matching on
//! prose would break the first time a message is reworded.
//!
//! So the module sets a stable error *name* on every failure and puts the
//! human-readable detail in the message. The names are published here so a host
//! matches on a constant.
//!
//! # The code a failure travels as
//!
//! A browser member replies with the same [`crate::DesktopResponse`] envelope
//! as a desktop member, and its [`crate::DesktopError::code`] is [`code`] of
//! the name: the desktop's own spelling wherever the meaning is the same
//! (`STALE_REF`, `ELEMENT_NOT_FOUND`, `TIMEOUT`, `POLICY_DENIED`,
//! `INVALID_ARGS`), so one handler serves both surfaces. The full name rides
//! along in `details.name`, and [`recovery`] fills the envelope's recovery
//! hint.

/// The prefix every error name in this contract begins with.
pub const PREFIX: &str = "ai.tinyhumans.tinycomputer.Browser.Error";

/// The request was malformed or self-contradictory: an unparseable URL, an
/// empty expression, a quality outside 1–100.
///
/// A model can act on this.
pub const INVALID_INPUT: &str = "ai.tinyhumans.tinycomputer.Browser.Error.InvalidInput";

/// The named session does not exist, or has been closed.
///
/// A host should open a new one rather than retrying.
pub const NO_SUCH_SESSION: &str = "ai.tinyhumans.tinycomputer.Browser.Error.NoSuchSession";

/// No element matched the target.
///
/// A model can act on this: take a fresh snapshot and choose again.
pub const NO_SUCH_ELEMENT: &str = "ai.tinyhumans.tinycomputer.Browser.Error.NoSuchElement";

/// The ref belongs to an earlier snapshot of this page.
///
/// Distinct from [`NO_SUCH_ELEMENT`] because the remedy is exactly "snapshot
/// again", and saying so is more useful than "not found".
pub const STALE_REF: &str = "ai.tinyhumans.tinycomputer.Browser.Error.StaleRef";

/// The element was found but could not be acted on: covered by an overlay,
/// disabled, or outside the document.
///
/// The message names the obstruction where the browser could identify it.
pub const NOT_ACTIONABLE: &str = "ai.tinyhumans.tinycomputer.Browser.Error.NotActionable";

/// The operation ran out of time.
pub const TIMEOUT: &str = "ai.tinyhumans.tinycomputer.Browser.Error.Timeout";

/// The session's `allowed_origins` does not admit the destination.
///
/// Never retry this one: the answer will not change, and a host that retries
/// turns a refused navigation into a loop.
pub const BLOCKED_BY_POLICY: &str = "ai.tinyhumans.tinycomputer.Browser.Error.BlockedByPolicy";

/// No browser could be launched or reached.
///
/// A model cannot act on this — it is a host or deployment problem.
pub const BROWSER_UNAVAILABLE: &str = "ai.tinyhumans.tinycomputer.Browser.Error.BrowserUnavailable";

/// The page reported a JavaScript exception, or the browser rejected a command.
pub const PAGE_ERROR: &str = "ai.tinyhumans.tinycomputer.Browser.Error.PageError";

/// The named held output does not exist, or has expired.
pub const NO_SUCH_OUTPUT: &str = "ai.tinyhumans.tinycomputer.Browser.Error.NoSuchOutput";

/// A limit was reached: too many sessions, too many held outputs, or an output
/// larger than the module will hold.
pub const LIMIT_EXCEEDED: &str = "ai.tinyhumans.tinycomputer.Browser.Error.LimitExceeded";

/// Everything else.
pub const MODULE_FAILED: &str = "ai.tinyhumans.tinycomputer.Browser.Error.ModuleFailed";

/// Every error name this contract defines.
pub const NAMES: &[&str] = &[
    INVALID_INPUT,
    NO_SUCH_SESSION,
    NO_SUCH_ELEMENT,
    STALE_REF,
    NOT_ACTIONABLE,
    TIMEOUT,
    BLOCKED_BY_POLICY,
    BROWSER_UNAVAILABLE,
    PAGE_ERROR,
    NO_SUCH_OUTPUT,
    LIMIT_EXCEEDED,
    MODULE_FAILED,
];

/// Whether `name` is one an agent can plausibly recover from by choosing
/// differently, as opposed to one that needs an operator.
///
/// This is the single decision a host tool makes on every failure, so it lives
/// in the contract rather than being re-derived — differently — by each caller.
///
/// # Examples
///
/// ```
/// # use tinycomputer_bus::browser::errors;
/// assert!(errors::is_agent_recoverable(errors::NO_SUCH_ELEMENT));
/// assert!(!errors::is_agent_recoverable(errors::BROWSER_UNAVAILABLE));
/// ```
#[must_use]
pub fn is_agent_recoverable(name: &str) -> bool {
    matches!(
        name,
        INVALID_INPUT | NO_SUCH_ELEMENT | STALE_REF | NOT_ACTIONABLE | TIMEOUT | PAGE_ERROR
    )
}

/// The envelope code a failure named `name` travels as.
///
/// Where the desktop members already have a code for the same situation this
/// is that code, so a caller driving both surfaces matches one vocabulary. A
/// name this build does not know is `INTERNAL`, as [`MODULE_FAILED`] is.
///
/// # Examples
///
/// ```
/// # use tinycomputer_bus::browser::errors;
/// assert_eq!(errors::code(errors::STALE_REF), "STALE_REF");
/// assert_eq!(errors::code(errors::NO_SUCH_ELEMENT), "ELEMENT_NOT_FOUND");
/// assert_eq!(errors::code(errors::BLOCKED_BY_POLICY), "POLICY_DENIED");
/// ```
#[must_use]
pub fn code(name: &str) -> &'static str {
    match name {
        INVALID_INPUT => "INVALID_ARGS",
        NO_SUCH_SESSION => "SESSION_NOT_FOUND",
        NO_SUCH_ELEMENT => "ELEMENT_NOT_FOUND",
        STALE_REF => "STALE_REF",
        NOT_ACTIONABLE => "NOT_ACTIONABLE",
        TIMEOUT => "TIMEOUT",
        BLOCKED_BY_POLICY => "POLICY_DENIED",
        BROWSER_UNAVAILABLE => "BROWSER_UNAVAILABLE",
        PAGE_ERROR => "PAGE_ERROR",
        NO_SUCH_OUTPUT => "OUTPUT_NOT_FOUND",
        LIMIT_EXCEEDED => "LIMIT_EXCEEDED",
        _ => "INTERNAL",
    }
}

/// The machine-readable way out of a failure named `name`, when there is one.
///
/// The strategies are the desktop's where they mean the same thing
/// (`refresh_snapshot_then_retry_original`), so an agent recovering from a
/// stale ref does not care which surface it was on.
///
/// A failure that may have come after the action reached the page — a
/// timeout, a page error — is `inspect_state_then_retry_original`, never a
/// blind retry: a click or a form submission may already have happened, and
/// repeating it could duplicate the effect. Check the page first.
///
/// # Examples
///
/// ```
/// # use tinycomputer_bus::browser::errors;
/// let hint = errors::recovery(errors::STALE_REF).expect("a stale ref has a way out");
/// assert!(hint.retryable && hint.requires_fresh_snapshot);
/// assert!(errors::recovery(errors::BLOCKED_BY_POLICY).is_none());
/// ```
#[must_use]
pub fn recovery(name: &str) -> Option<crate::RecoveryHint> {
    let hint = |strategy: &str, retryable, requires_fresh_snapshot| crate::RecoveryHint {
        strategy: strategy.to_owned(),
        retryable,
        requires_fresh_snapshot,
        retry_after_ms: None,
    };
    match name {
        STALE_REF => Some(hint("refresh_snapshot_then_retry_original", true, true)),
        NO_SUCH_ELEMENT => Some(hint("refresh_snapshot_then_choose_again", true, true)),
        NOT_ACTIONABLE => Some(hint("inspect_state_then_retry_original", true, true)),
        TIMEOUT | PAGE_ERROR => Some(hint("inspect_state_then_retry_original", true, true)),
        INVALID_INPUT => Some(hint("fix_request_then_retry", false, false)),
        NO_SUCH_SESSION => Some(hint("open_session_then_retry_original", false, false)),
        NO_SUCH_OUTPUT => Some(hint("capture_again_then_read", false, false)),
        _ => None,
    }
}

#[cfg(test)]
mod errors_tests;
