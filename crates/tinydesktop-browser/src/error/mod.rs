//! The crate-wide error type, and how each variant reaches a host.
//!
//! # One enum, and why the variants are what they are
//!
//! The variants are not a taxonomy of where a failure happened inside this
//! crate — a host cannot use that. They are a taxonomy of *what a caller should
//! do next*, which is the only distinction that survives the trip across the
//! bus: fix the request, take a fresh snapshot, give up and tell an operator.
//!
//! Each one maps to exactly one name in [`tinybrowser_bus::errors`], and
//! [`Error::wire_name`] is that mapping. It lives here rather than in the bus
//! adapter so a new variant cannot be added without deciding what a host sees.

use tinybrowser_bus::errors;

/// The result type every fallible public function in this crate returns.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong driving a browser.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A request was malformed: an unparseable URL, an empty expression, a
    /// quality outside 1–100.
    #[error("invalid input: {message}")]
    InvalidInput {
        /// What was wrong with it.
        message: String,
    },

    /// The named session does not exist, or has been closed.
    #[error("no such session: {id}")]
    NoSuchSession {
        /// The identity that was asked for.
        id: String,
    },

    /// Nothing matched the target.
    #[error("no element matched {target}")]
    NoSuchElement {
        /// The target as it was named.
        target: String,
    },

    /// The ref came from an earlier snapshot of this page.
    #[error("ref @{reference} is from snapshot {minted}, the page is now at snapshot {current}")]
    StaleRef {
        /// The ref that was used.
        reference: String,
        /// The snapshot that minted it.
        minted: u64,
        /// The snapshot the page is on now.
        current: u64,
    },

    /// The element was found but could not be acted on.
    #[error("element is not actionable: {reason}")]
    NotActionable {
        /// Why not — covered by which element, disabled, or off-document.
        reason: String,
    },

    /// An operation ran out of time.
    #[error("{operation} timed out after {elapsed_ms}ms")]
    Timeout {
        /// What was being attempted.
        operation: String,
        /// How long it was given.
        elapsed_ms: u64,
    },

    /// The session's origin allowlist does not admit the destination.
    #[error("navigation to {url} is not permitted by this session's allowed origins")]
    BlockedByPolicy {
        /// The destination that was refused.
        url: String,
    },

    /// No browser could be launched or reached.
    #[error("browser unavailable: {message}")]
    BrowserUnavailable {
        /// What was tried, and how it failed.
        message: String,
    },

    /// The page raised a JavaScript exception, or the browser rejected a
    /// command.
    #[error("page error: {message}")]
    PageError {
        /// The exception text or protocol error.
        message: String,
    },

    /// The named held output does not exist, or has expired.
    #[error("no such output: {id}")]
    NoSuchOutput {
        /// The identity that was asked for.
        id: String,
    },

    /// A bound was reached.
    #[error("limit exceeded: {message}")]
    LimitExceeded {
        /// Which bound, and what it is.
        message: String,
    },

    /// The connection to the browser is gone.
    ///
    /// Separate from [`Error::BrowserUnavailable`] because it says the session
    /// is dead rather than that a browser could not be found: the remedy is to
    /// open a new session, not to check the deployment.
    #[error("browser connection lost: {message}")]
    ConnectionLost {
        /// What the transport reported.
        message: String,
    },

    /// Anything else.
    #[error("{message}")]
    ModuleFailed {
        /// What happened.
        message: String,
    },
}

impl Error {
    /// The wire error name a host sees for this failure.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinybrowser::{errors, Error};
    /// let error = Error::invalid_input("empty expression");
    /// assert_eq!(error.wire_name(), errors::INVALID_INPUT);
    /// ```
    #[must_use]
    pub fn wire_name(&self) -> &'static str {
        match self {
            Self::InvalidInput { .. } => errors::INVALID_INPUT,
            // A lost connection is a dead session from the host's point of
            // view, and `NoSuchSession` is the name that tells it to open a new
            // one rather than retry into a socket that will never answer.
            Self::NoSuchSession { .. } | Self::ConnectionLost { .. } => errors::NO_SUCH_SESSION,
            Self::NoSuchElement { .. } => errors::NO_SUCH_ELEMENT,
            Self::StaleRef { .. } => errors::STALE_REF,
            Self::NotActionable { .. } => errors::NOT_ACTIONABLE,
            Self::Timeout { .. } => errors::TIMEOUT,
            Self::BlockedByPolicy { .. } => errors::BLOCKED_BY_POLICY,
            Self::BrowserUnavailable { .. } => errors::BROWSER_UNAVAILABLE,
            Self::PageError { .. } => errors::PAGE_ERROR,
            Self::NoSuchOutput { .. } => errors::NO_SUCH_OUTPUT,
            Self::LimitExceeded { .. } => errors::LIMIT_EXCEEDED,
            Self::ModuleFailed { .. } => errors::MODULE_FAILED,
        }
    }

    /// Builds an [`Error::InvalidInput`].
    #[must_use]
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput {
            message: message.into(),
        }
    }

    /// Builds an [`Error::PageError`].
    #[must_use]
    pub fn page(message: impl Into<String>) -> Self {
        Self::PageError {
            message: message.into(),
        }
    }

    /// Builds an [`Error::NotActionable`].
    #[must_use]
    pub fn not_actionable(reason: impl Into<String>) -> Self {
        Self::NotActionable {
            reason: reason.into(),
        }
    }

    /// Builds an [`Error::BrowserUnavailable`].
    #[must_use]
    pub fn browser_unavailable(message: impl Into<String>) -> Self {
        Self::BrowserUnavailable {
            message: message.into(),
        }
    }

    /// Builds an [`Error::ConnectionLost`].
    #[must_use]
    pub fn connection_lost(message: impl Into<String>) -> Self {
        Self::ConnectionLost {
            message: message.into(),
        }
    }

    /// Builds an [`Error::ModuleFailed`].
    #[must_use]
    pub fn failed(message: impl Into<String>) -> Self {
        Self::ModuleFailed {
            message: message.into(),
        }
    }

    /// Builds an [`Error::Timeout`] for `operation` given `elapsed_ms`.
    #[must_use]
    pub fn timeout(operation: impl Into<String>, elapsed_ms: u64) -> Self {
        Self::Timeout {
            operation: operation.into(),
            elapsed_ms,
        }
    }
}

#[cfg(test)]
mod test;
