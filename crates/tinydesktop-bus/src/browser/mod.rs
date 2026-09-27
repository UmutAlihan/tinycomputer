//! The browser interface's vocabulary: every type that crosses
//! [`names::INTERFACE`], and the names of the members that carry them.
//!
//! The module serves browser automation beside desktop automation: open a
//! session, navigate, snapshot the accessibility tree, act on a ref, read the
//! page, screenshot it, close. The engine behind it is
//! [agent-browser](https://github.com/vercel-labs/agent-browser), linked as a
//! library; these types were ported from the `tinybrowser-bus` crate, which
//! this interface supersedes, so a host written against `tinybrowser` needs
//! only the new interface name (`docs/specs/unified-agent.md`).
//!
//! # What is here
//!
//! - [`names`] — the interface name, the object path, and one constant per
//!   member, plus [`names::METHODS`] listing them in dispatch order.
//! - [`session`] — opening, listing, and closing the browser a host drives.
//! - [`page`] — navigating, extracting a page as text, evaluating JavaScript.
//! - [`snapshot`] — the accessibility tree, and the refs that address it.
//! - [`action`] — every interaction, and the three ways to name an element.
//! - [`download`] — browser download events retained as waitable handles.
//! - [`output`] — screenshots, and the handle protocol that carries them.
//! - [`errors`] — the failure names, and which of them an agent can act on.
//!
//! The contract version is the crate's own [`crate::CONTRACT_VERSION`]: the
//! desktop and browser interfaces ship in one module and evolve together.
//!
//! # Why these types are namespaced
//!
//! Several names — [`SnapshotRequest`], [`ScreenshotRequest`] — also exist for
//! the desktop interface with different shapes. Keeping the browser vocabulary
//! under `tinydesktop_bus::browser` means neither side shadows the other.
//!
//! # Example
//!
//! Building the frame bodies for the loop an agent actually runs — open,
//! navigate, snapshot, click — without a bus in sight:
//!
//! ```
//! use tinydesktop_bus::browser::{
//!     names, Action, NavigateRequest, SessionId, SessionOptions, SnapshotRequest, Target,
//! };
//!
//! let session = SessionId::new("s-1");
//!
//! let open = serde_json::to_value((SessionOptions::default(),))?;
//! let navigate = serde_json::to_value((&session, NavigateRequest::new("https://example.com")))?;
//! let snapshot = serde_json::to_value((&session, SnapshotRequest::interactive()))?;
//! let click = serde_json::to_value((
//!     &session,
//!     Action::Click { target: Target::parse("@e12"), new_tab: false },
//! ))?;
//!
//! assert_eq!(names::methods::OPEN_SESSION, "OpenSession");
//! assert_eq!(navigate[1]["wait_until"], "load");
//! assert_eq!(snapshot[1]["interactive_only"], true);
//! assert_eq!(click[1]["target"], serde_json::json!({ "kind": "ref", "value": "e12" }));
//! # let _ = open;
//! # Ok::<(), serde_json::Error>(())
//! ```

pub mod action;
pub mod download;
pub mod errors;
pub mod names;
pub mod output;
pub mod page;
pub mod session;
pub mod snapshot;

pub use action::{Action, ActionOutcome, LocateBy, Locator, ScrollDirection, Target, WaitState};
pub use download::{DownloadId, DownloadInfo, DownloadState, DownloadWaitRequest};
pub use output::{ImageFormat, OutputChunk, OutputId, OutputRef, ScreenshotRequest};
pub use page::{
    EvaluateRequest, NavigateRequest, PageState, PageText, ReadFormat, ReadRequest, WaitUntil,
};
pub use session::{SessionId, SessionInfo, SessionOptions, Viewport};
pub use snapshot::{ElementRef, Snapshot, SnapshotRequest};
