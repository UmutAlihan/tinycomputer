//! Tests for the desktop surface: snapshot parsing, window choice, the
//! clipboard round trip, and failing closed without touching input.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;
use tinycomputer_bus::{DesktopResponse, JevOperation};
use tinycomputer_core::surface::{
    Candidate, Depth, Screen, Surface, deliver_text, describe, fingerprint,
};

use super::{
    Restore, execute_desktop, front_of, observe, parse_reply, platform_combo, restore_plan,
    running_is_launched, screen_bounds, with_restoration,
};

fn parsed(tree: &serde_json::Value) -> Screen {
    parse_reply(
        &crate::Desktop::new(),
        "App",
        Some("@s:root"),
        DesktopResponse::ok("snapshot", json!({"app": "App", "tree": tree.clone()})),
    )
    .expect("synthetic snapshot parses")
}

#[test]
fn parsing_keeps_enabled_actionable_nodes_and_describes_them() {
    let screen = parsed(&json!({"role": "window", "children": [
        {"ref_id": "@s:e1", "role": "button", "name": "Play", "available_actions": ["Click"], "children_count": 4},
        {"ref_id": "@s:e2", "role": "button", "name": "Disabled", "available_actions": ["Click"], "states": ["disabled"]}
    ]}));
    assert_eq!(screen.candidates.len(), 1);
    assert_eq!(
        describe(&screen.candidates[0], false)["untrusted_accessibility_data"]["contains"],
        json!(4)
    );
    assert!(fingerprint(&screen).contains("Play"));
}

#[test]
fn static_text_is_kept_as_context_and_large_screens_are_not_cut() {
    let mut children = vec![
        json!({"role": "statictext", "name": "New Message"}),
        json!({"role": "statictext", "value": "Now   playing"}),
        json!({"role": "statictext", "name": "New Message"}),
        json!({"role": "group"}),
    ];
    children.extend((0..300).map(|index| {
        json!({"ref_id": format!("@s:e{index}"), "role": "button",
               "name": format!("b{index}"), "available_actions": ["Click"]})
    }));
    let screen = parsed(&json!({"role": "window", "children": children}));
    assert_eq!(screen.context, vec!["New Message", "Now playing"]);
    assert_eq!(screen.candidates.len(), 300, "narrowing, not truncation");
}

#[test]
fn a_rich_text_body_never_reaches_context_but_stays_in_text_nodes() {
    // A mail body or a web view holds its text over ref-less static text
    // descendants. That text is field content, so it must never surface in
    // `context` — sent to Jev on every request regardless of
    // `include_values` — even though it carries no ref and would otherwise
    // be treated like ordinary screen chrome.
    let screen = parsed(&json!({"role": "window", "children": [
        {"role": "statictext", "name": "New Message"},
        {"role": "webarea", "name": "message body", "children": [
            {"role": "statictext", "value": "Hi Sam, this is private."}
        ]}
    ]}));
    assert_eq!(screen.context, vec!["New Message", "message body"]);
    assert!(
        screen.text_nodes.iter().any(|node| node
            .value
            .as_ref()
            .and_then(serde_json::Value::as_str)
            == Some("Hi Sam, this is private.")),
        "the body text must still be reachable for gated field-content extraction"
    );
}

#[test]
fn a_token_fields_chip_labels_never_reach_context_but_stay_in_text_nodes() {
    // A token field (a mail recipient list) turns each typed address into an
    // attachment and exposes it as a ref-less static-text sibling right after
    // the field itself, rather than nested inside a rich-text area. That
    // sibling is field content too, so it must never surface in `context`
    // unconditionally, even though it carries no `webarea`/`document`
    // ancestor for `remembers_as_field_content` to recognize.
    let screen = parsed(&json!({"role": "window", "children": [
        {"ref_id": "@s:to", "role": "textfield", "name": "To", "available_actions": ["SetValue"]},
        {"role": "statictext", "name": "sam@example.com"}
    ]}));
    assert!(
        !screen
            .context
            .iter()
            .any(|line| line.contains("sam@example.com")),
        "a token field's chip label must not leak into unconditional context: {:?}",
        screen.context
    );
    assert!(
        screen
            .text_nodes
            .iter()
            .any(|node| node.name.as_deref() == Some("sam@example.com")),
        "the chip label must still be reachable for gated field-content extraction"
    );
}

#[test]
fn a_truncated_subtree_is_recorded_for_exploration() {
    let screen = parsed(&json!({"role": "window", "children": [
        {"ref_id": "@s:list", "role": "scrollarea", "subtree_truncated": true, "available_actions": ["Scroll"]},
        {"role": "group", "subtree_truncated": true}
    ]}));
    assert_eq!(screen.unexplored, ["@s:list"]);
}

#[test]
fn overlays_values_bounds_and_failed_observations_are_handled() {
    let screen = parse_reply(
        &crate::Desktop::new(),
        "App",
        None,
        DesktopResponse::ok(
            "snapshot",
            json!({"app": "App", "tree": {"role": "sheet", "children": [{
                "ref_id": "@s:e1", "role": "textfield", "value": "private",
                "available_actions": ["SetValue"], "states": ["focused"],
                "bounds": {"x": 1.0, "y": 2.0}
            }]}}),
        ),
    )
    .expect("an overlay that cannot be re-scoped stays usable");
    let with_values = describe(&screen.candidates[0], true);
    assert!(
        with_values["untrusted_accessibility_data"]
            .get("holds")
            .is_some()
    );
    assert!(
        with_values["untrusted_accessibility_data"]
            .get("state")
            .is_some()
    );
    let unnamed = Candidate {
        role: "button".to_owned(),
        bounds: Some(json!({"x": 1.0, "y": 2.0})),
        ..Candidate::default()
    };
    assert!(
        describe(&unnamed, false)["untrusted_accessibility_data"]
            .get("bounds")
            .is_some()
    );

    let missing = "__tinycomputer_missing__";
    assert!(
        !observe(&crate::Desktop::new(), missing, None, Depth::Skeleton)
            .expect_err("missing app fails")
            .ok
    );
    assert!(observe(&crate::Desktop::new(), missing, Some("@s:e1"), Depth::Full).is_err());
    for role in ["alert", "menu", "popover"] {
        let screen = parse_reply(
            &crate::Desktop::new(),
            missing,
            None,
            DesktopResponse::ok(
                "snapshot",
                json!({"app": "App", "tree": {"role": role, "children": []}}),
            ),
        )
        .expect("an overlay that cannot be re-scoped stays usable");
        assert_eq!(screen.surface, "window");
    }
    let failed = DesktopResponse::err(
        "snapshot",
        tinycomputer_bus::DesktopError::new("FAIL", "failed"),
    );
    assert!(parse_reply(&crate::Desktop::new(), "App", Some("@s:root"), failed).is_err());
    let no_data = DesktopResponse {
        version: tinycomputer_bus::ENVELOPE_VERSION.to_owned(),
        ok: true,
        command: "snapshot".to_owned(),
        data: None,
        error: None,
    };
    assert!(parse_reply(&crate::Desktop::new(), "App", Some("@s:root"), no_data).is_err());
}

#[test]
fn traversal_is_bounded_in_depth_and_breadth() {
    let mut deep = json!({"ref_id": "@s:deep", "role": "button", "available_actions": ["Click"]});
    for _ in 0..66 {
        deep = json!({"role": "group", "children": [deep]});
    }
    assert!(parsed(&deep).candidates.is_empty());
    let many = (0..4_100)
        .map(|index| json!({"role": "group", "name": format!("node-{index}")}))
        .collect::<Vec<_>>();
    let wide = parsed(&json!({"role": "window", "children": many}));
    assert!(wide.candidates.is_empty());
    assert_eq!(wide.context.len(), 60, "context is capped");
}

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

#[test]
fn an_app_with_several_windows_counts_as_launched() {
    let ambiguous = DesktopResponse::err(
        "launch",
        tinycomputer_bus::DesktopError::new("AMBIGUOUS_TARGET", "several windows"),
    );
    assert!(running_is_launched(ambiguous).ok);
    let missing = DesktopResponse::err(
        "launch",
        tinycomputer_bus::DesktopError::new("APP_NOT_FOUND", "no such app"),
    );
    assert!(!running_is_launched(missing).ok);
}

#[test]
fn the_desktop_backend_fails_closed_on_empty_targets_without_touching_input() {
    // Every call names nothing, so each fails before pressing, pasting, or
    // launching anything on the machine running the tests.
    let desktop = crate::Desktop::new();
    let empty = Candidate::default();
    assert!(Surface::read_value(&desktop, &empty).is_none());
    assert!(!Surface::paste(&desktop, "", &empty, "text").ok);
    assert!(!Surface::press(&desktop, "", "").ok);
    assert!(!Surface::launch(&desktop, "").ok);
    assert!(Surface::observe(&desktop, "__tinycomputer_missing__", None, Depth::Full).is_err());
    assert!(!deliver_text(&desktop, "", &empty, "text").ok);
}

#[test]
fn every_closed_operation_dispatches_without_panicking() {
    let desktop = crate::Desktop::new();
    let candidate = Candidate::default();
    for operation in [
        JevOperation::Click,
        JevOperation::TypeText,
        JevOperation::Check,
        JevOperation::Uncheck,
        JevOperation::Expand,
        JevOperation::Collapse,
        JevOperation::Scroll,
        JevOperation::Wait,
        JevOperation::Drill,
        JevOperation::Widen,
        JevOperation::Done,
        JevOperation::Blocked,
    ] {
        let reply = execute_desktop(
            &desktop,
            operation,
            Some(&candidate),
            Some("text".to_owned()),
        );
        assert!(!reply.command.is_empty());
    }
}

#[test]
fn the_front_window_is_the_focused_then_the_first_visible_titled_one() {
    let windows = [
        json!({"id": "w-1", "title": "", "visible": true}),
        json!({"id": "w-2", "title": "Desktop", "visible": true}),
        json!({"id": "w-3", "title": "Other", "visible": false, "is_focused": true}),
    ];
    assert_eq!(front_of(&windows).as_deref(), Some("w-3"));
    assert_eq!(front_of(&windows[..2]).as_deref(), Some("w-2"));
    assert_eq!(front_of(&windows[..1]).as_deref(), Some("w-1"));
    assert!(front_of(&[]).is_none());
}

#[derive(Clone, Default)]
struct Drawn(std::sync::Arc<std::sync::Mutex<Vec<tinycomputer_cursor::OverlayCommand>>>);

impl tinycomputer_cursor::OverlaySink for Drawn {
    fn send(&mut self, command: &tinycomputer_cursor::OverlayCommand) -> std::io::Result<()> {
        self.0.lock().unwrap().push(command.clone());
        Ok(())
    }
}

fn boxed(bounds: serde_json::Value) -> Candidate {
    Candidate {
        ref_id: "@s:e1".to_owned(),
        role: "button".to_owned(),
        bounds: Some(bounds),
        ..Candidate::default()
    }
}

#[test]
fn a_candidates_screen_bounds_are_read_when_complete_and_positive() {
    let rect = screen_bounds(&boxed(
        json!({"x": 10.0, "y": 20.0, "width": 80.0, "height": 24.0}),
    ))
    .expect("complete bounds parse");
    assert!((rect.width - 80.0).abs() < f64::EPSILON && (rect.y - 20.0).abs() < f64::EPSILON);
    for broken in [
        json!({"x": 10.0, "y": 20.0}),
        json!({"x": 10.0, "y": 20.0, "width": 0.0, "height": 24.0}),
        json!("somewhere"),
    ] {
        assert!(screen_bounds(&boxed(broken)).is_none());
    }
    assert!(screen_bounds(&Candidate::default()).is_none());
}

#[test]
fn a_pointer_operation_glides_the_cursor_onto_its_target_first() {
    use tinycomputer_cursor::{CursorPace, OverlayCommand, ScreenCursor};
    let drawn = Drawn::default();
    let cursor =
        ScreenCursor::with_sink(CursorPace::Natural, Box::new(drawn.clone())).without_waiting();
    let desktop = crate::Desktop::new().with_cursor(std::sync::Arc::new(cursor));
    let target = boxed(json!({"x": 400.0, "y": 300.0, "width": 120.0, "height": 32.0}));

    // Headless and permission-less, the action itself fails closed as before;
    // the cursor has already glided onto where it would land.
    let _clicked = execute_desktop(&desktop, JevOperation::Click, Some(&target), None);
    let _scrolled = execute_desktop(&desktop, JevOperation::Scroll, Some(&target), None);
    let _unboxed = execute_desktop(
        &desktop,
        JevOperation::Check,
        Some(&Candidate::default()),
        None,
    );
    let sent = drawn.0.lock().unwrap();
    assert_eq!(sent.len(), 1, "only the boxed pointer operation glides");
    let OverlayCommand::Glide { path, .. } = &sent[0] else {
        panic!("a glide");
    };
    let [_, x, y] = *path.last().unwrap();
    assert!((400.0..=520.0).contains(&x) && (300.0..=332.0).contains(&y));
}
