//! Stable download payload definitions.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Chrome's stable identity for one download.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DownloadId(String);

impl DownloadId {
    /// Build an id from the Chrome download guid.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The guid as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DownloadId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<String> for DownloadId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for DownloadId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Chrome's current state for a download.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    /// Bytes are still arriving.
    #[default]
    InProgress,
    /// Chrome reported the file complete.
    Completed,
    /// Chrome cancelled the transfer.
    Cancelled,
}

impl DownloadState {
    /// Whether no further progress event is expected.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

/// A retained download event handle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DownloadInfo {
    /// Monotonic order within the session, starting at one.
    pub sequence: u64,
    /// Chrome's stable download guid.
    pub id: DownloadId,
    /// Source URL that initiated the download, when reported.
    pub url: String,
    /// Filename proposed by the server or page.
    pub suggested_filename: String,
    /// Current transfer state.
    pub state: DownloadState,
    /// Bytes Chrome reports as received.
    pub received_bytes: u64,
    /// Expected total bytes when Chrome knows it.
    pub total_bytes: Option<u64>,
    /// Expected absolute local path when the session configured a download
    /// directory and the suggested filename was safe.
    pub path: Option<String>,
}

/// How long to wait for the next unreported terminal download.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadWaitRequest {
    /// Deadline in milliseconds. Defaults to the session deadline.
    pub timeout_ms: Option<u64>,
}
