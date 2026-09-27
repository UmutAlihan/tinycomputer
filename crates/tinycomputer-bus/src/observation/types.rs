//! Payloads for `Snapshot`, `Find`, `Get`, `Is`, and `Screenshot`.

use serde::{Deserialize, Serialize};

use crate::vocabulary::{ElementProperty, ElementStateProperty, StatePredicate, Surface};

/// The argument to [`crate::names::methods::SNAPSHOT`].
///
/// Every field has a default, so the empty object snapshots the focused
/// application's focused window at full depth.
///
/// `skeleton` is the token-budget lever: it caps the walk at three levels and
/// returns structure without leaf detail, which is enough to decide where to
/// look before spending a full walk on that subtree. Follow it with a second
/// snapshot carrying `root_ref` set to the interesting container.
// The engine's own snapshot and find arguments are flag-shaped, and this is
// the wire form a caller writes by hand or an agent emits as a tool call.
// Folding the flags into enums would rename them on the wire for a tidiness
// this file cannot afford: the names are the contract.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapshotRequest {
    /// The application to walk. Absent means the focused application.
    pub app: Option<String>,
    /// The window to walk. Absent means the application's focused window.
    pub window_id: Option<String>,
    /// How many levels to descend. Absent means the engine default.
    pub max_depth: Option<u8>,
    /// Whether to include each element's bounding rectangle.
    pub include_bounds: bool,
    /// Whether to return only elements a caller could act on.
    pub interactive_only: bool,
    /// Whether to drop presentational detail from the returned tree.
    pub compact: bool,
    /// The region of the tree to walk.
    pub surface: Surface,
    /// Whether to return a depth-capped structural overview instead of a full
    /// walk.
    pub skeleton: bool,
    /// A ref to root the walk at, from an earlier snapshot. Cannot be combined
    /// with `skeleton`, which always starts at the surface root.
    pub root_ref: Option<String>,
    /// The snapshot a bare `root_ref` belongs to. Unnecessary for a qualified
    /// ref, which names its own snapshot.
    pub snapshot_id: Option<String>,
}

/// The argument to [`crate::names::methods::FIND`].
///
/// The filter fields are combined with AND: an element matches when it
/// satisfies every field that is present. `exact` switches the text fields
/// from substring matching to equality.
///
/// The selection fields are mutually exclusive — supplying more than one of
/// `count`, `first`, `last`, and `nth` is an `INVALID_ARGS` error rather than
/// a silent precedence rule.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FindRequest {
    /// The application to search. Absent means the focused application.
    pub app: Option<String>,
    /// The window to search. Absent means the application's focused window.
    pub window_id: Option<String>,
    /// A ref to root the search at, from an earlier snapshot.
    pub root: Option<String>,
    /// The snapshot a bare `root` belongs to.
    pub snapshot: Option<String>,
    /// The region of the tree to search.
    pub surface: Surface,
    /// The accessibility role to require, for example `button`.
    pub role: Option<String>,
    /// The accessible name to require.
    pub name: Option<String>,
    /// The accessible description to require.
    pub description: Option<String>,
    /// The platform-native identifier to require.
    pub native_id: Option<String>,
    /// The element value to require.
    pub value: Option<String>,
    /// Text anywhere in the element to require.
    pub text: Option<String>,
    /// Whether the text fields must match exactly rather than as substrings.
    pub exact: bool,
    /// State tokens every match must satisfy.
    pub states: Vec<StatePredicate>,
    /// Return the number of matches instead of the matches themselves.
    pub count: bool,
    /// Return only the first match.
    pub first: bool,
    /// Return only the last match.
    pub last: bool,
    /// Return only the match at this zero-based position.
    pub nth: Option<usize>,
    /// Return at most this many matches.
    pub limit: Option<usize>,
}

/// The argument to [`crate::names::methods::GET`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetRequest {
    /// The ref to read, from an earlier snapshot or find.
    pub ref_id: String,
    /// The snapshot a bare `ref_id` belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    /// The property to read.
    pub property: ElementProperty,
}

impl GetRequest {
    /// Builds a request reading `property` from `ref_id`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_bus::{ElementProperty, GetRequest};
    /// let request = GetRequest::new("@s8f3k2p9:e1", ElementProperty::Value);
    /// assert!(request.snapshot_id.is_none());
    /// ```
    #[must_use]
    pub fn new(ref_id: impl Into<String>, property: ElementProperty) -> Self {
        Self {
            ref_id: ref_id.into(),
            snapshot_id: None,
            property,
        }
    }
}

/// The argument to [`crate::names::methods::IS`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsRequest {
    /// The ref to test, from an earlier snapshot or find.
    pub ref_id: String,
    /// The snapshot a bare `ref_id` belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    /// The state to test.
    pub property: ElementStateProperty,
}

impl IsRequest {
    /// Builds a request testing `property` on `ref_id`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_bus::{ElementStateProperty, IsRequest};
    /// let request = IsRequest::new("@s8f3k2p9:e1", ElementStateProperty::Enabled);
    /// assert_eq!(request.ref_id, "@s8f3k2p9:e1");
    /// ```
    #[must_use]
    pub fn new(ref_id: impl Into<String>, property: ElementStateProperty) -> Self {
        Self {
            ref_id: ref_id.into(),
            snapshot_id: None,
            property,
        }
    }
}

/// The argument to [`crate::names::methods::SCREENSHOT`].
///
/// With `output_path` the image is written there and the reply carries the
/// path; without it the reply carries the image base64-encoded. Prefer the
/// path for anything larger than a single control: a full-screen PNG inlined
/// into a bus frame is megabytes of base64 that a caller usually only wants to
/// hand to a file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScreenshotRequest {
    /// The application to capture. Absent, with no other target, captures the
    /// main display.
    pub app: Option<String>,
    /// The window to capture.
    pub window_id: Option<String>,
    /// The zero-based display to capture.
    pub screen: Option<usize>,
    /// Where to write the image instead of inlining it.
    pub output_path: Option<String>,
}
