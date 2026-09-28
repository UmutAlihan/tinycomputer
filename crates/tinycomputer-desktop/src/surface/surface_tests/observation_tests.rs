//! Tests for snapshot parsing, context and field content, bounded traversal,
//! and the choice of front window.

use serde_json::json;
use tinycomputer_bus::DesktopResponse;
use tinycomputer_core::surface::{Candidate, Depth, Screen, describe, fingerprint};

use crate::surface::observation::{front_of, observe, parse_reply};

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
