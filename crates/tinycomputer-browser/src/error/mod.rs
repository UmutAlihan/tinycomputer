//! The crate-wide error type, and how each variant reaches a host.
//!
//! # One enum, and why the variants are what they are
//!
//! The variants are not a taxonomy of where a failure happened inside this
//! crate — a host cannot use that. They are a taxonomy of *what a caller should
//! do next*, which is the only distinction that survives the trip across the
//! bus: fix the request, take a fresh snapshot, give up and tell an operator.
//!
//! Each one maps to exactly one name in [`tinycomputer_bus::browser::errors`], and
//! [`Error::wire_name`] is that mapping. It lives here rather than in the bus
//! adapter so a new variant cannot be added without deciding what a host sees.

use tinycomputer_bus::browser::errors;
use tinycomputer_bus::{Delivery, DeliveryDisposition, DesktopError};

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

    /// The ref came from an earlier snapshot of this page, or from none.
    ///
    /// agent-browser re-mints refs with every snapshot, so the remedy is always
    /// the same: take a fresh snapshot and choose again.
    #[error("ref @{reference} is not in the latest snapshot, take a fresh one")]
    StaleRef {
        /// The ref that was used, without its `@`.
        reference: String,
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
    /// # use tinycomputer_browser::{errors, Error};
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

    /// This failure as the envelope error a bus member replies with.
    ///
    /// The code is [`errors::code`] of the wire name — the desktop's spelling
    /// where the meaning is shared — the recovery hint is
    /// [`errors::recovery`]'s, and the full wire name rides in
    /// `details.name` for a host that matches on it. A failure refused before
    /// the browser was asked anything is marked not delivered, so a caller
    /// knows retrying it cannot repeat an effect.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_browser::Error;
    /// let error = Error::StaleRef { reference: "e3".to_owned() }.envelope();
    /// assert_eq!(error.code, "STALE_REF");
    /// assert!(error.recovery.is_some_and(|hint| hint.requires_fresh_snapshot));
    /// ```
    #[must_use]
    pub fn envelope(&self) -> DesktopError {
        let name = self.wire_name();
        let mut error = DesktopError::new(errors::code(name), self.to_string());
        error.recovery = errors::recovery(name);
        error.details = Some(serde_json::json!({
            "name": name,
            "agent_recoverable": errors::is_agent_recoverable(name),
        }));
        error.suggestion = self.suggestion().map(str::to_owned);
        if self.refused_before_delivery() {
            error.disposition = Delivery::of(DeliveryDisposition::NotDelivered);
        }
        error
    }

    /// What a caller should do next, in one sentence, when that is known.
    fn suggestion(&self) -> Option<&'static str> {
        match self {
            Self::StaleRef { .. } => {
                Some("take a fresh BrowserSnapshot and use a ref from it")
            }
            Self::NoSuchElement { .. } => {
                Some("take a fresh BrowserSnapshot and choose a target that is on the page")
            }
            Self::NotActionable { .. } => {
                Some("dismiss whatever covers the element, or scroll it into view, then retry")
            }
            Self::NoSuchSession { .. } | Self::ConnectionLost { .. } => {
                Some("open a new session with BrowserOpenSession")
            }
            Self::BlockedByPolicy { .. } => {
                Some("do not retry; the session's allowed origins refuse this destination")
            }
            Self::NoSuchOutput { .. } => {
                Some("capture the screenshot again; held outputs expire after five minutes")
            }
            Self::LimitExceeded { .. } => {
                Some("close sessions or release outputs you no longer need")
            }
            Self::InvalidInput { .. }
            | Self::Timeout { .. }
            | Self::BrowserUnavailable { .. }
            | Self::PageError { .. }
            | Self::ModuleFailed { .. } => None,
        }
    }

    /// Whether this failure was decided before any command reached the
    /// browser.
    fn refused_before_delivery(&self) -> bool {
        matches!(
            self,
            Self::InvalidInput { .. }
                | Self::NoSuchSession { .. }
                | Self::StaleRef { .. }
                | Self::BlockedByPolicy { .. }
                | Self::NoSuchOutput { .. }
                | Self::LimitExceeded { .. }
        )
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
