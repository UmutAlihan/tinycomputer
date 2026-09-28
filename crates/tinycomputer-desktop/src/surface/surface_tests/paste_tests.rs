//! Tests for the clipboard round trip around a paste.

use serde_json::json;
use tinycomputer_bus::DesktopResponse;

use crate::surface::act::platform_combo;
use crate::surface::paste::{Restore, restore_plan, with_restoration};

#[test]
fn restore_plan_writes_back_whatever_flavor_the_pasteboard_held() {
    let text = DesktopResponse::ok("clipboard-get", json!({"type": "text", "text": "hello"}));
    assert_eq!(
        restore_plan(&text),
        Some(Restore::Set(tinycomputer_bus::ClipboardSetRequest::text(
            "hello"
        )))
    );

    let file_urls = DesktopResponse::ok(
        "clipboard-get",
        json!({"type": "file_urls", "file_urls": ["file:///tmp/a.txt"]}),
    );
    assert_eq!(
        restore_plan(&file_urls),
        Some(Restore::Set(tinycomputer_bus::ClipboardSetRequest {
            file_urls: vec!["file:///tmp/a.txt".to_owned()],
            ..tinycomputer_bus::ClipboardSetRequest::default()
        }))
    );

    let image = DesktopResponse::ok(
        "clipboard-get",
        json!({"type": "image", "path": "/tmp/clip.png"}),
    );
    assert_eq!(
        restore_plan(&image),
        Some(Restore::Set(tinycomputer_bus::ClipboardSetRequest {
            image: Some("/tmp/clip.png".to_owned()),
            ..tinycomputer_bus::ClipboardSetRequest::default()
        }))
    );
}

#[test]
fn restore_plan_clears_an_originally_empty_pasteboard() {
    let empty = DesktopResponse::ok("clipboard-get", json!({"type": "text", "found": false}));
    assert_eq!(restore_plan(&empty), Some(Restore::Clear));
}

#[test]
fn restore_plan_leaves_the_pasteboard_alone_when_the_read_failed() {
    let failed = DesktopResponse::err(
        "clipboard-get",
        tinycomputer_bus::DesktopError::new("PERM_DENIED", "no automation permission"),
    );
    assert_eq!(restore_plan(&failed), None);
}

#[test]
fn platform_combo_keeps_cmd_on_macos_and_maps_it_to_ctrl_elsewhere() {
    // Flows and the paste path only ever write app shortcuts in terms of
    // `cmd`; the engine's own combo parser maps that literally to the Meta
    // key, which is the Windows/Super key everywhere but macOS.
    assert_eq!(platform_combo("cmd+n", true), "cmd+n");
    assert_eq!(platform_combo("cmd+shift+n", true), "cmd+shift+n");
    assert_eq!(platform_combo("cmd+n", false), "ctrl+n");
    assert_eq!(platform_combo("cmd+shift+n", false), "ctrl+shift+n");
    assert_eq!(platform_combo("cmd+a", false), "ctrl+a");
    assert_eq!(platform_combo("cmd+v", false), "ctrl+v");
    // Combos with no `cmd` modifier are left untouched on every platform.
    assert_eq!(platform_combo("escape", false), "escape");
    assert_eq!(platform_combo("tab", true), "tab");
}

#[test]
fn a_failed_clipboard_restoration_is_folded_into_the_response_instead_of_ignored() {
    let ok = DesktopResponse::ok("press", json!({}));
    // A successful restoration leaves the response untouched.
    assert_eq!(with_restoration(ok.clone(), true), ok);
    // A failed restoration is reported, not silently dropped, without
    // turning a delivered field's own success into a failure.
    let annotated = with_restoration(ok, false);
    assert!(annotated.ok, "the field operation itself still succeeded");
    assert_eq!(annotated.data, Some(json!({"clipboard_restored": false})));
    // A failed operation's own error is left alone: nothing to fold into.
    let failed = DesktopResponse::err(
        "press",
        tinycomputer_bus::DesktopError::new("PERM_DENIED", "no automation permission"),
    );
    assert_eq!(with_restoration(failed.clone(), false), failed);
}
