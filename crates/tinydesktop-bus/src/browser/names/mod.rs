//! The bus identity of the tinydesktop browser interface: interface name, object path, and
//! one constant per member.
//!
//! Nothing here is a string literal at a call site. A host names a member
//! through [`methods`] and the object through [`OBJECT_PATH`], so a rename is a
//! compile error in every consumer rather than a runtime "unknown method".
//!
//! [`METHODS`] is kept in the same order as the interface's dispatch table in
//! `crates/tinydesktop/src/tinybus_module`, and that crate asserts the two
//! agree, so a member added to one and forgotten in the other fails the build.

/// The well-known interface name the module claims on the bus.
pub const INTERFACE: &str = "ai.tinyhumans.tinydesktop.Browser";

/// The object path the module serves its interface at.
pub const OBJECT_PATH: &str = "/ai/tinyhumans/tinydesktop/Browser";

/// One constant per member of [`INTERFACE`].
pub mod methods {
    /// Launches or attaches a browser and returns the session that owns it.
    ///
    /// Takes a [`crate::browser::SessionOptions`] and returns a [`crate::browser::SessionInfo`].
    pub const OPEN_SESSION: &str = "OpenSession";

    /// Closes a session and everything it owns.
    ///
    /// Takes a [`crate::browser::SessionId`] and returns nothing. Closing a session that
    /// is already gone succeeds: a host retrying a close must not have to
    /// distinguish "never existed" from "already cleaned up".
    pub const CLOSE_SESSION: &str = "CloseSession";

    /// Lists the sessions this module is currently holding open.
    ///
    /// Takes nothing and returns a `Vec<`[`crate::browser::SessionInfo`]`>`.
    pub const LIST_SESSIONS: &str = "ListSessions";

    /// Navigates the session's active page.
    ///
    /// Takes a [`crate::browser::SessionId`] and a [`crate::browser::NavigateRequest`], and
    /// returns the [`crate::browser::PageState`] the navigation settled on.
    pub const NAVIGATE: &str = "Navigate";

    /// Captures the accessibility tree of the active page, with element refs.
    ///
    /// Takes a [`crate::browser::SessionId`] and a [`crate::browser::SnapshotRequest`], and
    /// returns a [`crate::browser::Snapshot`]. The refs it hands back are what
    /// [`PERFORM`] resolves as [`crate::browser::Target::Ref`].
    pub const SNAPSHOT: &str = "Snapshot";

    /// Performs one interaction against the active page.
    ///
    /// Takes a [`crate::browser::SessionId`] and an [`crate::browser::Action`], and returns an
    /// [`crate::browser::ActionOutcome`].
    pub const PERFORM: &str = "Perform";

    /// Extracts the active page as agent-readable text.
    ///
    /// Takes a [`crate::browser::SessionId`] and a [`crate::browser::ReadRequest`], and returns a
    /// [`crate::browser::PageText`].
    pub const READ_PAGE: &str = "ReadPage";

    /// Evaluates JavaScript in the active page and returns its value.
    ///
    /// Takes a [`crate::browser::SessionId`] and an [`crate::browser::EvaluateRequest`], and
    /// returns the resolved value as arbitrary JSON.
    pub const EVALUATE: &str = "Evaluate";

    /// Captures a screenshot and holds it for collection.
    ///
    /// Takes a [`crate::browser::SessionId`] and a [`crate::browser::ScreenshotRequest`], and
    /// returns an [`crate::browser::OutputRef`] naming the held image. The image itself
    /// is pulled with [`READ_OUTPUT`] — see [`crate::browser::output`] for why it is not
    /// returned inline.
    pub const SCREENSHOT: &str = "Screenshot";

    /// Reads one chunk of a held output.
    ///
    /// Takes an output id, a byte offset, and a maximum length, and returns an
    /// [`crate::browser::OutputChunk`].
    pub const READ_OUTPUT: &str = "ReadOutput";

    /// Releases a held output before it expires.
    ///
    /// Takes an output id and returns nothing. Releasing an output that is
    /// already gone succeeds, for the same reason [`CLOSE_SESSION`] does.
    pub const RELEASE_OUTPUT: &str = "ReleaseOutput";

    /// Lists retained downloads for one browser session.
    ///
    /// Takes a [`crate::browser::SessionId`] and returns a `Vec<`[`crate::browser::DownloadInfo`]`>`.
    pub const LIST_DOWNLOADS: &str = "ListDownloads";

    /// Waits for the next terminal download not returned by an earlier wait.
    ///
    /// Takes a [`crate::browser::SessionId`] and [`crate::browser::DownloadWaitRequest`], and
    /// returns a [`crate::browser::DownloadInfo`].
    pub const WAIT_DOWNLOAD: &str = "WaitDownload";

    /// Reports the contract version the module serves.
    ///
    /// Takes nothing and returns `(u32, u32)`. A host compares it with
    /// [`crate::is_compatible`] before its first real call.
    pub const CONTRACT_VERSION: &str = "ContractVersion";
}

/// Every member of [`INTERFACE`], in the order the interface dispatches them.
///
/// `crates/tinydesktop` asserts its declared manifest methods against this
/// list, so the two cannot drift.
pub const METHODS: &[&str] = &[
    methods::OPEN_SESSION,
    methods::CLOSE_SESSION,
    methods::LIST_SESSIONS,
    methods::NAVIGATE,
    methods::SNAPSHOT,
    methods::PERFORM,
    methods::READ_PAGE,
    methods::EVALUATE,
    methods::SCREENSHOT,
    methods::READ_OUTPUT,
    methods::RELEASE_OUTPUT,
    methods::LIST_DOWNLOADS,
    methods::WAIT_DOWNLOAD,
    methods::CONTRACT_VERSION,
];

#[cfg(test)]
mod test;
