//! Payloads for the synthesized keyboard and mouse members.

use serde::{Deserialize, Serialize};

use crate::vocabulary::{Modifier, MouseButton};

/// The argument to [`crate::names::methods::PRESS`].
///
/// The combo is written the way a menu writes it: `cmd+shift+p`, `ctrl+c`,
/// `escape`, `f5`. The engine normalizes and validates it, and refuses combos
/// the platform reserves unless `force` is set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PressRequest {
    /// The key combination to press.
    pub combo: String,
    /// The application to press it at. Absent sends it to whatever holds
    /// focus.
    pub app: Option<String>,
    /// Whether to send a combo the platform would otherwise refuse.
    pub force: bool,
}

impl PressRequest {
    /// Builds a request pressing `combo` at whatever holds focus.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_bus::PressRequest;
    /// let request = PressRequest::new("cmd+shift+p");
    /// assert!(request.app.is_none() && !request.force);
    /// ```
    #[must_use]
    pub fn new(combo: impl Into<String>) -> Self {
        Self {
            combo: combo.into(),
            app: None,
            force: false,
        }
    }
}

/// The argument to [`crate::names::methods::KEY_DOWN`] and
/// [`crate::names::methods::KEY_UP`].
///
/// Both members validate this payload and then fail closed; see the
/// [module documentation](crate::input) for why.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HoldKeyRequest {
    /// The key combination to hold or release.
    pub combo: String,
    /// Whether to accept a combo the platform would otherwise refuse.
    pub force: bool,
}

/// The argument to [`crate::names::methods::HOVER`].
///
/// Address the target either by `ref_id` or by `x`/`y`, not both. `duration_ms`
/// is accepted and rejected: a stateless call cannot own the cursor for a dwell,
/// so the engine tells the caller to hover and then wait instead of pretending
/// otherwise.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HoverRequest {
    /// The ref to hover over.
    pub ref_id: Option<String>,
    /// The snapshot a bare `ref_id` belongs to.
    pub snapshot_id: Option<String>,
    /// The screen x coordinate to hover at.
    pub x: Option<f64>,
    /// The screen y coordinate to hover at.
    pub y: Option<f64>,
    /// How long to dwell. Any positive value is rejected.
    pub duration_ms: Option<u64>,
    /// How long to keep retrying ref resolution before giving up.
    pub timeout_ms: Option<u64>,
}

/// One end of a [`DragRequest`], addressed by ref or by coordinates.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DragEndpoint {
    /// The ref at this end of the drag.
    pub ref_id: Option<String>,
    /// The screen x coordinate at this end of the drag.
    pub x: Option<f64>,
    /// The screen y coordinate at this end of the drag.
    pub y: Option<f64>,
}

impl DragEndpoint {
    /// Builds an endpoint at `ref_id`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_bus::DragEndpoint;
    /// assert_eq!(DragEndpoint::at_ref("@s1:e2").ref_id.as_deref(), Some("@s1:e2"));
    /// ```
    #[must_use]
    pub fn at_ref(ref_id: impl Into<String>) -> Self {
        Self {
            ref_id: Some(ref_id.into()),
            x: None,
            y: None,
        }
    }

    /// Builds an endpoint at the screen point `x`, `y`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_bus::DragEndpoint;
    /// assert_eq!(DragEndpoint::at_point(10.0, 20.0).y, Some(20.0));
    /// ```
    #[must_use]
    pub fn at_point(x: f64, y: f64) -> Self {
        Self {
            ref_id: None,
            x: Some(x),
            y: Some(y),
        }
    }
}

/// The argument to [`crate::names::methods::DRAG`].
///
/// This is the one member that holds a mouse button down, and it can only do so
/// because it owns both ends of the gesture within a single call.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DragRequest {
    /// Where the drag starts.
    pub from: DragEndpoint,
    /// Where the drag ends.
    pub to: DragEndpoint,
    /// The snapshot bare refs in either endpoint belong to.
    pub snapshot_id: Option<String>,
    /// How long the movement between the endpoints should take.
    pub duration_ms: Option<u64>,
    /// How long to hold at the destination before releasing, for a drop target
    /// that needs a moment to accept.
    pub drop_delay_ms: Option<u64>,
    /// How long to keep retrying ref resolution before giving up.
    pub timeout_ms: Option<u64>,
}

/// The argument to [`crate::names::methods::MOUSE_MOVE`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MouseMoveRequest {
    /// The screen x coordinate to move to.
    pub x: f64,
    /// The screen y coordinate to move to.
    pub y: f64,
}

/// The argument to [`crate::names::methods::MOUSE_CLICK`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MouseClickRequest {
    /// The screen x coordinate to click at.
    pub x: f64,
    /// The screen y coordinate to click at.
    pub y: f64,
    /// Which button to click.
    pub button: MouseButton,
    /// How many clicks. `0` is treated as one; the engine caps the upper end.
    pub count: u32,
    /// Modifier keys to hold for the click.
    pub modifiers: Vec<Modifier>,
}

/// The argument to [`crate::names::methods::MOUSE_DOWN`] and
/// [`crate::names::methods::MOUSE_UP`].
///
/// Both members validate this payload and then fail closed; see the
/// [module documentation](crate::input) for why.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HoldMouseRequest {
    /// The screen x coordinate of the press or release.
    pub x: f64,
    /// The screen y coordinate of the press or release.
    pub y: f64,
    /// Which button.
    pub button: MouseButton,
    /// Modifier keys to hold.
    pub modifiers: Vec<Modifier>,
}

/// The argument to [`crate::names::methods::MOUSE_WHEEL`].
///
/// Deltas are in lines, positive being down and right. Every coordinate and
/// delta must be finite.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MouseWheelRequest {
    /// The screen x coordinate to scroll at.
    pub x: f64,
    /// The screen y coordinate to scroll at.
    pub y: f64,
    /// Vertical lines to scroll.
    pub dy: f64,
    /// Horizontal lines to scroll.
    pub dx: f64,
    /// Modifier keys to hold for the scroll.
    pub modifiers: Vec<Modifier>,
}
