//! Payloads for the application and window members.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The argument to [`crate::names::methods::LAUNCH`].
///
/// `attach_if_running` defaults to `true`, so launching an application that is
/// already open attaches to it. Set it to `false` to require a fresh process;
/// the engine then fails with a structured error naming the running pid rather
/// than quietly attaching to something the caller did not start.
///
/// `cdp_port` opens a Chrome `DevTools` Protocol port on a Chromium-based
/// application and verifies it before returning. That is the seam for driving
/// web contents with a browser automation library while native menus, dialogs,
/// and windows stay on the accessibility path. `0` asks the engine to resolve a
/// free port and report the one it chose.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchRequest {
    /// The application to launch.
    pub app: String,
    /// Arguments to pass to the process.
    pub args: Vec<String>,
    /// Environment variables to set for the process.
    pub env: BTreeMap<String, String>,
    /// The working directory to start it in.
    pub cwd: Option<String>,
    /// How long to wait for the application to be ready. Absent means the
    /// engine default.
    pub timeout_ms: Option<u64>,
    /// Whether to attach to an already-running instance instead of failing.
    /// Absent means `true`.
    pub attach_if_running: Option<bool>,
    /// Whether to bring the application forward so it presents a window. A
    /// document-based application creates its first window in response to
    /// activation, so a caller that needs a window has to ask for one.
    pub activate: bool,
    /// The `DevTools` protocol port to open, or `0` to let the engine pick.
    pub cdp_port: Option<u16>,
}

impl LaunchRequest {
    /// Builds a request launching `app` with every option at its default.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_bus::LaunchRequest;
    /// let request = LaunchRequest::new("Safari");
    /// assert_eq!(request.app, "Safari");
    /// assert!(request.attach_if_running.is_none());
    /// ```
    #[must_use]
    pub fn new(app: impl Into<String>) -> Self {
        Self {
            app: app.into(),
            ..Self::default()
        }
    }
}

/// The argument to [`crate::names::methods::CLOSE_APP`].
///
/// The engine refuses to close a process it considers protected — a window
/// server, a login agent — whatever `force` says.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CloseAppRequest {
    /// The application to close.
    pub app: String,
    /// Whether to terminate rather than asking the application to quit.
    pub force: bool,
}

/// The argument to [`crate::names::methods::LIST_APPS`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ListAppsRequest {
    /// Return only applications whose name contains this. Absent returns
    /// every running application.
    pub app: Option<String>,
}

/// The argument to [`crate::names::methods::LIST_WINDOWS`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ListWindowsRequest {
    /// Return only this application's windows. Absent returns every window.
    pub app: Option<String>,
}

/// The argument to [`crate::names::methods::LIST_SURFACES`].
///
/// Reports which of the [`crate::Surface`] variants the platform can currently
/// address for an application, which is how a caller discovers whether a menu
/// or a popover is reachable before asking for one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ListSurfacesRequest {
    /// The application to report surfaces for. Absent means the focused
    /// application.
    pub app: Option<String>,
}

/// The argument to the members that operate on a window as a whole.
///
/// Shared by `Minimize`, `Maximize`, and `Restore`. Absent fields mean the
/// focused application's focused window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowRequest {
    /// The application owning the window.
    pub app: Option<String>,
    /// The window itself.
    pub window_id: Option<String>,
}

/// The argument to [`crate::names::methods::FOCUS_WINDOW`].
///
/// The three fields narrow the same set. A combination matching more than one
/// window fails with `AMBIGUOUS_TARGET` listing the candidates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FocusWindowRequest {
    /// The window to focus.
    pub window_id: Option<String>,
    /// The application owning it.
    pub app: Option<String>,
    /// The window title to match.
    pub title: Option<String>,
}

/// The argument to [`crate::names::methods::RESIZE_WINDOW`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResizeWindowRequest {
    /// The application owning the window.
    pub app: Option<String>,
    /// The window to resize.
    pub window_id: Option<String>,
    /// The new width in points.
    pub width: f64,
    /// The new height in points.
    pub height: f64,
}

/// The argument to [`crate::names::methods::MOVE_WINDOW`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MoveWindowRequest {
    /// The application owning the window.
    pub app: Option<String>,
    /// The window to move.
    pub window_id: Option<String>,
    /// The new screen x coordinate of the window's origin.
    pub x: f64,
    /// The new screen y coordinate of the window's origin.
    pub y: f64,
}
