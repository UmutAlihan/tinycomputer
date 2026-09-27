//! The payload for `Wait`.

use serde::{Deserialize, Serialize};

/// The argument to [`crate::names::methods::WAIT`].
///
/// Exactly one of `ms`, `element`, `window`, `text`, `surface`, and `event`
/// selects what is being waited for. Supplying none, or more than one, is an
/// `INVALID_ARGS` error.
///
/// A wait that runs out of time fails with `TIMEOUT`; it does not return
/// successfully having waited in vain.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WaitRequest {
    /// Sleep for this many milliseconds. The unconditional mode.
    pub ms: Option<u64>,
    /// Wait for this ref to satisfy `predicate`.
    pub element: Option<String>,
    /// Wait for a window whose title contains this.
    pub window: Option<String>,
    /// Wait for a notification containing this text.
    pub text: Option<String>,
    /// Wait for a surface change: `menu`, `menu_closed`, or `notification`.
    pub surface: Option<String>,
    /// Wait for a named accessibility event.
    pub event: Option<String>,
    /// The window the `event` mode watches.
    pub window_id: Option<String>,
    /// The snapshot a bare `element` ref belongs to.
    pub snapshot_id: Option<String>,
    /// The state the `element` mode waits for, for example `enabled` or
    /// `gone`.
    pub predicate: Option<String>,
    /// The value the `element` mode waits for the element to hold.
    pub value: Option<String>,
    /// The action the `event` mode waits for.
    pub action: Option<String>,
    /// How many occurrences to wait for.
    pub count: Option<usize>,
    /// The application to scope the wait to.
    pub app: Option<String>,
    /// How long to wait before failing with `TIMEOUT`. Absent means the engine
    /// default.
    pub timeout_ms: Option<u64>,
}

impl WaitRequest {
    /// Builds a request that sleeps for `ms` milliseconds.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_bus::WaitRequest;
    /// assert_eq!(WaitRequest::sleep(250).ms, Some(250));
    /// ```
    #[must_use]
    pub fn sleep(ms: u64) -> Self {
        Self {
            ms: Some(ms),
            ..Self::default()
        }
    }
}
