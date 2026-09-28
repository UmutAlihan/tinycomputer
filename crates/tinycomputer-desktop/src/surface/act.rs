//! Acting: the closed operations turned into agent-desktop commands, launch and
//! key presses included.

use serde_json::{Value, json};
use tinycomputer_bus::{
    ClipboardFormat, ClipboardGetRequest, ClipboardSetRequest, DesktopResponse, Direction,
    ElementProperty, GetRequest, JevOperation, LaunchRequest, ListWindowsRequest, PressRequest,
    RefRequest, ScrollRequest, SetValueRequest, SnapshotRequest, Surface as Overlay, WaitRequest,
};
use tinycomputer_core::surface::{Candidate, Depth, Screen, Surface, uses_pointer};
use tinycomputer_cursor::Rect;

use crate::Desktop;

/// Treats "several windows match" as launched: the application is running,
/// and which of its windows to act in is the next step's decision.
pub(crate) fn running_is_launched(reply: DesktopResponse) -> DesktopResponse {
    if reply
        .error
        .as_ref()
        .is_some_and(|error| error.code == "AMBIGUOUS_TARGET")
    {
        return DesktopResponse::ok(
            &reply.command,
            json!({"running": true, "windows": "several"}),
        );
    }
    reply
}

/// Rewrites the platform-neutral `cmd` modifier a flow's shortcuts and this
/// module's paste path both write into whatever modifier this platform
/// actually binds for an application's primary shortcuts.
///
/// The engine's combo parser maps `cmd` literally to the Meta key
/// (`vendor/agent-desktop/crates/core/src/commands/combo.rs`), which is the
/// Command key on macOS but the Windows/Super key everywhere else, so a
/// shortcut such as `cmd+n` would open the Start menu instead of a new item
/// on Windows and Linux. `is_macos` is threaded through rather than read from
/// `cfg!` here so the mapping stays a pure, unit-testable function; the one
/// caller below supplies the real platform.
pub(crate) fn platform_combo(combo: &str, is_macos: bool) -> String {
    if is_macos {
        return combo.to_owned();
    }
    combo
        .split('+')
        .map(|part| if part == "cmd" { "ctrl" } else { part })
        .collect::<Vec<_>>()
        .join("+")
}

fn press_at(app: &str, combo: &str) -> PressRequest {
    let mut request = PressRequest::new(platform_combo(combo, cfg!(target_os = "macos")));
    request.app = Some(app.to_owned());
    request
}

/// A candidate's bounds, which agent-desktop reports in global screen points.
pub(crate) fn screen_bounds(target: &Candidate) -> Option<Rect> {
    let bounds = target.bounds.as_ref()?;
    let field = |name: &str| bounds.get(name).and_then(Value::as_f64);
    let rect = Rect::new(field("x")?, field("y")?, field("width")?, field("height")?);
    (rect.is_valid() && rect.width > 0.0 && rect.height > 0.0).then_some(rect)
}

pub(crate) fn execute_desktop(
    desktop: &Desktop,
    operation: JevOperation,
    target: Option<&Candidate>,
    text: Option<String>,
) -> DesktopResponse {
    if let (Some(cursor), Some(bounds)) = (
        desktop.cursor(),
        target
            .filter(|_| uses_pointer(operation))
            .and_then(screen_bounds),
    ) {
        cursor.arrive(bounds);
    }
    let ref_id = target.map(|node| node.ref_id.clone());
    match operation {
        JevOperation::Click => desktop.click(RefRequest::new(ref_id.unwrap_or_default())),
        JevOperation::TypeText => desktop.set_value(SetValueRequest {
            ref_id: ref_id.unwrap_or_default(),
            value: text.unwrap_or_default(),
            ..SetValueRequest::default()
        }),
        JevOperation::Check => desktop.check(RefRequest::new(ref_id.unwrap_or_default())),
        JevOperation::Uncheck => desktop.uncheck(RefRequest::new(ref_id.unwrap_or_default())),
        JevOperation::Expand => desktop.expand(RefRequest::new(ref_id.unwrap_or_default())),
        JevOperation::Collapse => desktop.collapse(RefRequest::new(ref_id.unwrap_or_default())),
        JevOperation::Scroll => desktop.scroll(ScrollRequest::new(
            ref_id.unwrap_or_default(),
            Direction::Down,
            3,
        )),
        JevOperation::Wait => desktop.wait(WaitRequest::sleep(500)),
        JevOperation::Drill | JevOperation::Widen => {
            DesktopResponse::ok("look", json!({"root": ref_id}))
        }
        JevOperation::Done | JevOperation::Blocked => {
            DesktopResponse::ok("resolve-intent", json!({}))
        }
    }
}
