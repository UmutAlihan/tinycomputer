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

/// The prefix every error name in this contract begins with.
pub const PREFIX: &str = "ai.tinyhumans.tinybrowser.Error";

/// The request was malformed or self-contradictory: an unparseable URL, an
/// empty expression, a quality outside 1–100.
///
/// A model can act on this.
pub const INVALID_INPUT: &str = "ai.tinyhumans.tinybrowser.Error.InvalidInput";

/// The named session does not exist, or has been closed.
///
/// A host should open a new one rather than retrying.
pub const NO_SUCH_SESSION: &str = "ai.tinyhumans.tinybrowser.Error.NoSuchSession";

/// No element matched the target.
///
/// A model can act on this: take a fresh snapshot and choose again.
pub const NO_SUCH_ELEMENT: &str = "ai.tinyhumans.tinybrowser.Error.NoSuchElement";

/// The ref belongs to an earlier snapshot of this page.
///
/// Distinct from [`NO_SUCH_ELEMENT`] because the remedy is exactly "snapshot
/// again", and saying so is more useful than "not found".
pub const STALE_REF: &str = "ai.tinyhumans.tinybrowser.Error.StaleRef";

/// The element was found but could not be acted on: covered by an overlay,
/// disabled, or outside the document.
///
/// The message names the obstruction where the browser could identify it.
pub const NOT_ACTIONABLE: &str = "ai.tinyhumans.tinybrowser.Error.NotActionable";

/// The operation ran out of time.
pub const TIMEOUT: &str = "ai.tinyhumans.tinybrowser.Error.Timeout";

/// The session's `allowed_origins` does not admit the destination.
///
/// Never retry this one: the answer will not change, and a host that retries
/// turns a refused navigation into a loop.
pub const BLOCKED_BY_POLICY: &str = "ai.tinyhumans.tinybrowser.Error.BlockedByPolicy";

/// No browser could be launched or reached.
///
/// A model cannot act on this — it is a host or deployment problem.
pub const BROWSER_UNAVAILABLE: &str = "ai.tinyhumans.tinybrowser.Error.BrowserUnavailable";

/// The page reported a JavaScript exception, or the browser rejected a command.
pub const PAGE_ERROR: &str = "ai.tinyhumans.tinybrowser.Error.PageError";

/// The named held output does not exist, or has expired.
pub const NO_SUCH_OUTPUT: &str = "ai.tinyhumans.tinybrowser.Error.NoSuchOutput";

/// A limit was reached: too many sessions, too many held outputs, or an output
/// larger than the module will hold.
pub const LIMIT_EXCEEDED: &str = "ai.tinyhumans.tinybrowser.Error.LimitExceeded";

/// Everything else.
pub const MODULE_FAILED: &str = "ai.tinyhumans.tinybrowser.Error.ModuleFailed";

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
/// # use tinybrowser_bus::errors;
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

#[cfg(test)]
mod test;
