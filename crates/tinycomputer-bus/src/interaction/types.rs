//! Payloads for the ref-addressed action members.

use serde::{Deserialize, Serialize};

use crate::vocabulary::Direction;

/// The argument to every member whose action carries no data of its own.
///
/// Shared by `Click`, `DoubleClick`, `TripleClick`, `RightClick`, `Clear`,
/// `Focus`, `Toggle`, `Check`, `Uncheck`, `Expand`, `Collapse`, and
/// `ScrollTo`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RefRequest {
    /// The ref to act on, from an earlier snapshot or find.
    pub ref_id: String,
    /// The snapshot a bare `ref_id` belongs to. Unnecessary for a qualified
    /// ref.
    pub snapshot_id: Option<String>,
    /// How long to keep retrying resolution before giving up. Absent means the
    /// engine default.
    pub timeout_ms: Option<u64>,
}

impl RefRequest {
    /// Builds a request addressing `ref_id`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_bus::RefRequest;
    /// let request = RefRequest::new("@s8f3k2p9:e1");
    /// assert_eq!(request.ref_id, "@s8f3k2p9:e1");
    /// assert!(request.timeout_ms.is_none());
    /// ```
    #[must_use]
    pub fn new(ref_id: impl Into<String>) -> Self {
        Self {
            ref_id: ref_id.into(),
            snapshot_id: None,
            timeout_ms: None,
        }
    }
}

/// The argument to [`crate::names::methods::TYPE`].
///
/// The text is delivered to the element, not to whatever holds focus, so a
/// window that steals focus mid-run does not receive it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TypeRequest {
    /// The ref to type into.
    pub ref_id: String,
    /// The snapshot a bare `ref_id` belongs to.
    pub snapshot_id: Option<String>,
    /// The text to type. The engine rejects text past its own length cap.
    pub text: String,
    /// How long to keep retrying resolution before giving up.
    pub timeout_ms: Option<u64>,
}

/// The argument to [`crate::names::methods::SET_VALUE`].
///
/// Unlike [`TypeRequest`], this replaces the element's value in one step
/// rather than delivering keystrokes, so it does not fire the per-character
/// handlers a form may depend on. Use it for a field whose contents matter and
/// `Type` for a field whose typing matters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetValueRequest {
    /// The ref to write to.
    pub ref_id: String,
    /// The snapshot a bare `ref_id` belongs to.
    pub snapshot_id: Option<String>,
    /// The value to write.
    pub value: String,
    /// How long to keep retrying resolution before giving up.
    pub timeout_ms: Option<u64>,
}

/// The argument to [`crate::names::methods::SELECT`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectRequest {
    /// The ref of the list, menu, or picker to select within.
    pub ref_id: String,
    /// The snapshot a bare `ref_id` belongs to.
    pub snapshot_id: Option<String>,
    /// The option to select, by its accessible name.
    pub value: String,
    /// How long to keep retrying resolution before giving up.
    pub timeout_ms: Option<u64>,
}

/// The argument to [`crate::names::methods::SCROLL`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrollRequest {
    /// The ref of the scrollable container.
    pub ref_id: String,
    /// The snapshot a bare `ref_id` belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    /// Which way to scroll.
    pub direction: Direction,
    /// How far to scroll, in the platform's scroll units.
    pub amount: u32,
    /// How long to keep retrying resolution before giving up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

impl ScrollRequest {
    /// Builds a request scrolling `ref_id` by `amount` in `direction`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_bus::{Direction, ScrollRequest};
    /// let request = ScrollRequest::new("@s8f3k2p9:e4", Direction::Down, 3);
    /// assert_eq!(request.amount, 3);
    /// ```
    #[must_use]
    pub fn new(ref_id: impl Into<String>, direction: Direction, amount: u32) -> Self {
        Self {
            ref_id: ref_id.into(),
            snapshot_id: None,
            direction,
            amount,
            timeout_ms: None,
        }
    }
}
