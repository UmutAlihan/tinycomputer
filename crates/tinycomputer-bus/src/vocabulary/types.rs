//! Shared enumerations and the one shared struct used across request payloads.
//!
//! None of these carry `#[non_exhaustive]`, and that is deliberate. The module
//! crate converts each one into its `agent-desktop-core` counterpart with an
//! exhaustive `match`, so a variant added here without a conversion fails that
//! build. A `#[non_exhaustive]` enum would force a wildcard arm in its place,
//! and the wildcard would swallow exactly the case worth catching. Adding a
//! variant is a minor bump of [`crate::CONTRACT_VERSION`] either way.

use serde::{Deserialize, Serialize};

/// The region of an application's accessibility tree a command addresses.
///
/// [`Surface::Window`] is the ordinary case: the tree rooted at an application
/// window. The other variants address transient or system-owned regions — a
/// menu that is open right now, the notification centre, the dock — which are
/// not children of any window and therefore cannot be reached by descending
/// from one.
///
/// A platform serves only the surfaces its accessibility backend implements.
/// Asking for one it does not implement fails with `PLATFORM_NOT_SUPPORTED`
/// and lists the supported surfaces in the error details.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// The tree rooted at an application window. The default.
    #[default]
    Window,
    /// The tree rooted at whatever currently holds focus.
    Focused,
    /// An open menu.
    Menu,
    /// The application menu bar.
    Menubar,
    /// A sheet attached to a window.
    Sheet,
    /// A popover.
    Popover,
    /// A modal alert.
    Alert,
    /// The desktop root.
    Desktop,
    /// The taskbar.
    Taskbar,
    /// The system tray.
    SystemTray,
    /// The quick settings panel.
    QuickSettings,
    /// The notification centre.
    NotificationCenter,
    /// A toolbar.
    Toolbar,
    /// The dock.
    Dock,
    /// The Spotlight search surface.
    Spotlight,
    /// The menu bar extras region.
    MenuBarExtras,
    /// The system tray overflow region.
    SystemTrayOverflow,
    /// The start menu.
    StartMenu,
    /// The action centre.
    ActionCenter,
}

/// A scroll or navigation direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    /// Towards the top of the content.
    Up,
    /// Towards the bottom of the content.
    Down,
    /// Towards the start of the content.
    Left,
    /// Towards the end of the content.
    Right,
}

/// A physical mouse button.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    /// The primary button. The default.
    #[default]
    Left,
    /// The secondary button.
    Right,
    /// The middle button, usually the wheel.
    Middle,
}

/// A keyboard modifier key.
///
/// `Cmd` deserializes to [`Modifier::Meta`], matching the engine's alias, so a
/// caller may spell the macOS name and reach the same variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Modifier {
    /// Command on macOS, Windows key elsewhere.
    #[serde(alias = "Cmd")]
    Meta,
    /// Control.
    Ctrl,
    /// Option on macOS, Alt elsewhere.
    Alt,
    /// Shift.
    Shift,
}

/// The pasteboard flavor a clipboard read asks for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardFormat {
    /// Whichever flavor the pasteboard currently holds.
    Auto,
    /// Plain text. The default.
    #[default]
    Text,
    /// An image, written to a file and reported by path.
    Image,
    /// A list of file URLs.
    FileUrls,
}

/// A readable property of a resolved element.
///
/// Addressed by [`crate::names::methods::GET`], which returns the property name
/// alongside its value so a reply is self-describing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementProperty {
    /// The element's text content, read live where the platform allows it.
    Text,
    /// The element's value, read live where the platform allows it.
    Value,
    /// The element's accessible name.
    Title,
    /// The element's bounding rectangle in screen coordinates.
    Bounds,
    /// The element's accessibility role.
    Role,
    /// Every state token the element currently carries.
    States,
}

/// A boolean state of a resolved element.
///
/// Addressed by [`crate::names::methods::IS`]. A property that does not apply
/// to the element's role is reported as inapplicable rather than as `false`,
/// so a caller can tell "this checkbox is unchecked" from "this is not a thing
/// that can be checked".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementStateProperty {
    /// Whether the element is on screen and not hidden.
    Visible,
    /// Whether the element accepts interaction.
    Enabled,
    /// Whether a checkable element is checked.
    Checked,
    /// Whether the element holds keyboard focus.
    Focused,
    /// Whether an expandable element is expanded.
    Expanded,
    /// Whether a selectable element is selected.
    Selected,
}

/// A state token a [`crate::FindRequest`] requires, optionally negated.
///
/// `expected` defaults to `true` when absent, so `{"token": "enabled"}` means
/// "must be enabled" and `{"token": "enabled", "expected": false}` means "must
/// not be".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatePredicate {
    /// The state token, for example `enabled`, `focused`, or `checked`.
    pub token: String,
    /// The value the token must have. Absent means `true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<bool>,
}

impl StatePredicate {
    /// Builds a predicate requiring `token` to be set.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_bus::StatePredicate;
    /// assert_eq!(StatePredicate::set("enabled").expected, None);
    /// ```
    #[must_use]
    pub fn set(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            expected: None,
        }
    }

    /// Builds a predicate requiring `token` to hold `expected`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_bus::StatePredicate;
    /// assert_eq!(StatePredicate::expect("focused", false).expected, Some(false));
    /// ```
    #[must_use]
    pub fn expect(token: impl Into<String>, expected: bool) -> Self {
        Self {
            token: token.into(),
            expected: Some(expected),
        }
    }
}
