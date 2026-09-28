//! Tests for observing the desktop: snapshot requests, parsing the tree into
//! a screen, visible verification, and change detection.

use super::*;

#[test]
fn snapshot_request_binds_exact_window_id() {
    let request = snapshot_request("TextEdit", Some("w-515619"), None);
    assert_eq!(request.app.as_deref(), Some("TextEdit"));
    assert_eq!(request.window_id.as_deref(), Some("w-515619"));
    assert_eq!(snapshot_request("TextEdit", None, None).window_id, None);
}

#[test]
fn goal_verifies_visible_static_text_without_an_action_ref() {
    let reply = DesktopResponse::ok(
        "snapshot",
        json!({
            "app": "Calculator",
            "window": {"title": "Calculator"},
            "tree": {
                "role": "window",
                "name": "Calculator",
                "children": [{
                    "role": "scrollarea",
                    "name": "Edit field",
                    "ref_id": "@s:e1",
                    "available_actions": ["Scroll"],
                    "children": [{
                        "role": "statictext",
                        "name": "\u{200e}12",
                        "value": "\u{200e}12"
                    }]
                }]
            }
        }),
    );
    let screen = parse_reply(&crate::Desktop::new(), "Calculator", None, None, reply).unwrap();
    let evidence = super::verify::verify(
        &screen,
        &[VisiblePredicate::NamePresent {
            name: "\u{200e}12".to_owned(),
        }],
    );
    assert!(super::verify::satisfied(&evidence));
    assert_eq!(screen.candidates.len(), 1);
}

#[test]
fn screen_change_detection_ignores_ephemeral_refs_and_sees_visible_text() {
    let mut before = clickable_screen();
    before.observed = vec![Candidate {
        role: "statictext".into(),
        name: Some("Old result".into()),
        ..Candidate::default()
    }];
    let mut after = before.clone();
    after.candidates[0].ref_id = "@new-snapshot:e1".into();
    assert_eq!(fingerprint(&before), fingerprint(&after));
    after.observed[0].name = Some("New result".into());
    assert_ne!(fingerprint(&before), fingerprint(&after));
}

#[test]
fn screen_parsing_filters_disabled_nodes_and_builds_descriptions() {
    let reply = DesktopResponse::ok(
        "snapshot",
        json!({
            "app": "Spotify", "window": {"title": "Liked Songs"},
            "tree": {"role": "window", "children": [
                {"ref_id": "@s:e1", "role": "button", "name": "Play First Song by Artist", "available_actions": ["Click"], "children_count": 4},
                {"ref_id": "@s:e2", "role": "button", "name": "Disabled", "available_actions": ["Click"], "states": ["disabled"]}
            ]}
        }),
    );
    let screen = parse_reply(
        &crate::Desktop::new(),
        "Spotify",
        None,
        Some("@s:root"),
        reply,
    )
    .expect("synthetic snapshot parses");
    assert_eq!(screen.candidates.len(), 1);
    assert_eq!(
        describe(&screen.candidates[0], false)["untrusted_accessibility_data"]["contains"],
        json!(4)
    );
    let before = fingerprint(&screen);
    assert_eq!(before.len(), 16);
    let mut changed = screen.clone();
    changed
        .observed
        .iter_mut()
        .find(|node| node.name.as_deref() == Some("Play First Song by Artist"))
        .unwrap()
        .name = Some("Pause First Song by Artist".into());
    assert_ne!(before, fingerprint(&changed));
}

#[test]
fn textedit_native_identifier_survives_snapshot_and_is_offered_to_jev() {
    let reply = DesktopResponse::ok(
        "snapshot",
        json!({
            "app": "TextEdit", "window": {"title": "Untitled"},
            "tree": {"role": "window", "children": [{
                "ref_id": "@s:e1", "role": "text field", "name": null,
                "description": null,
                "native_id": {"kind": "ax_identifier", "value": "First Text View"},
                "available_actions": ["SetValue"], "value": ""
            }]}
        }),
    );
    let screen = parse_reply(&crate::Desktop::new(), "TextEdit", None, None, reply).unwrap();
    let field = &screen.candidates[0];
    assert_eq!(field.native_id.as_ref().unwrap().value, "First Text View");
    assert_eq!(
        target_payload(field).name.as_deref(),
        Some("First Text View")
    );
    assert!(
        describe(field, false)["untrusted_accessibility_data"]["what"]
            .as_str()
            .unwrap()
            .contains("First Text View")
    );
}

#[test]
fn screen_helpers_cover_overlay_values_bounds_and_failed_observation() {
    let reply = DesktopResponse::ok(
        "snapshot",
        json!({
            "app": "App",
            "tree": {"role": "sheet", "children": [{
                "ref_id": "@s:e1", "role": "textfield", "value": "private",
                "available_actions": ["SetValue"], "states": ["focused"],
                "bounds": {"x": 1.0, "y": 2.0}
            }]}
        }),
    );
    let screen = parse_reply(&crate::Desktop::new(), "App", None, None, reply)
        .expect("original synthetic overlay remains usable");
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

    let failed = observe(
        &crate::Desktop::new(),
        "__tinycomputer_missing__",
        None,
        None,
    )
    .expect_err("missing app fails");
    assert!(!failed.ok);
    assert!(
        observe(
            &crate::Desktop::new(),
            "__tinycomputer_missing__",
            None,
            Some("@s:e1")
        )
        .is_err()
    );

    for role in ["alert", "menu", "popover"] {
        let screen = parse_reply(
            &crate::Desktop::new(),
            "__tinycomputer_missing__",
            None,
            None,
            DesktopResponse::ok(
                "snapshot",
                json!({"app": "App", "tree": {"role": role, "children": []}}),
            ),
        )
        .expect("synthetic overlay remains usable");
        assert_eq!(screen.surface, "window");
    }

    let failed_reply = DesktopResponse::err(
        "snapshot",
        tinycomputer_bus::DesktopError::new("FAIL", "failed"),
    );
    assert!(
        parse_reply(
            &crate::Desktop::new(),
            "App",
            None,
            Some("@s:root"),
            failed_reply
        )
        .is_err()
    );
    let no_data = DesktopResponse {
        version: tinycomputer_bus::ENVELOPE_VERSION.to_owned(),
        ok: true,
        command: "snapshot".to_owned(),
        data: None,
        error: None,
    };
    assert!(
        parse_reply(
            &crate::Desktop::new(),
            "App",
            None,
            Some("@s:root"),
            no_data
        )
        .is_err()
    );
}

#[test]
fn accessibility_tree_traversal_is_bounded() {
    let mut deep = json!({
        "ref_id": "@s:deep",
        "role": "button",
        "available_actions": ["Click"]
    });
    for _ in 0..66 {
        deep = json!({"role": "group", "children": [deep]});
    }
    let bounded = parse_reply(
        &crate::Desktop::new(),
        "App",
        None,
        Some("@s:root"),
        DesktopResponse::ok("snapshot", json!({"app": "App", "tree": deep})),
    )
    .expect("deep tree is bounded");
    assert!(bounded.candidates.is_empty());

    let many = (0..4_100)
        .map(|index| json!({"role": "group", "name": format!("node-{index}")}))
        .collect::<Vec<_>>();
    let bounded = parse_reply(
        &crate::Desktop::new(),
        "App",
        None,
        Some("@s:root"),
        DesktopResponse::ok(
            "snapshot",
            json!({"app": "App", "tree": {"role": "window", "children": many}}),
        ),
    )
    .expect("wide tree is bounded");
    assert!(bounded.candidates.is_empty());
}
