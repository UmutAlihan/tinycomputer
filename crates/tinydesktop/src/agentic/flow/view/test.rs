//! Tests for what a flow sees: parsing, context, fingerprints, and labels.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;
use tinydesktop_bus::DesktopResponse;

use super::{
    Candidate, Depth, Screen, change_note, describe, destructive_label, exact_named_match,
    fingerprint, is_destructive, named_in_stop_before, observe, parse_reply, target_payload,
};

fn clickable_screen() -> Screen {
    Screen {
        app: "Spotify".to_owned(),
        window: Some("Liked Songs".to_owned()),
        surface: "window".to_owned(),
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
        candidates: vec![Candidate {
            ref_id: "@s1:e1".to_owned(),
            role: "button".to_owned(),
            name: Some("Play First Song by Artist".to_owned()),
            available_actions: vec!["Click".to_owned()],
            bounds: Some(json!({"x": 10.0, "y": 100.0})),
            ..Candidate::default()
        }],
    }
}

fn two_candidate_screen() -> Screen {
    let mut screen = clickable_screen();
    screen.candidates.push(Candidate {
        ref_id: "@s1:e2".to_owned(),
        role: "button".to_owned(),
        name: Some("Play Second Song by Artist".to_owned()),
        available_actions: vec!["Click".to_owned()],
        bounds: Some(json!({"x": 10.0, "y": 160.0})),
        ..Candidate::default()
    });
    screen
}

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
        !screen.context.iter().any(|line| line.contains("sam@example.com")),
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

    let missing = "__tinydesktop_missing__";
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
        tinydesktop_bus::DesktopError::new("FAIL", "failed"),
    );
    assert!(parse_reply(&crate::Desktop::new(), "App", Some("@s:root"), failed).is_err());
    let no_data = DesktopResponse {
        version: tinydesktop_bus::ENVELOPE_VERSION.to_owned(),
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
fn a_fingerprint_ignores_ref_churn_between_snapshots() {
    let first = clickable_screen();
    let mut second = clickable_screen();
    second.candidates[0].ref_id = "@s2:e9".to_owned();
    assert_eq!(fingerprint(&first), fingerprint(&second));
    second.candidates[0].states = vec!["selected".to_owned()];
    assert_ne!(fingerprint(&first), fingerprint(&second));
}

#[test]
fn a_change_note_names_what_appeared_and_what_went_away() {
    let before = clickable_screen();
    let mut after = two_candidate_screen();
    after.candidates.remove(0);
    after.window = Some("Other".to_owned());
    after.surface = "sheet".to_owned();
    after.context = (0..8).map(|index| format!("line {index}")).collect();
    let note = change_note(&before, &after, true);
    assert!(note.contains("window is now \"Other\""), "{note}");
    assert!(note.contains("surface is now sheet"), "{note}");
    assert!(note.contains("and 3 more"), "{note}");
    assert!(note.contains("gone: button \"Play First Song"), "{note}");
    assert_eq!(
        change_note(&before, &before, false),
        "nothing on screen changed"
    );
    let mut same_labels = clickable_screen();
    same_labels.candidates[0].states = vec!["selected".to_owned()];
    assert_eq!(
        change_note(&before, &same_labels, true),
        "the screen changed"
    );
}

#[test]
fn irreversible_labels_and_exact_names_are_recognised() {
    assert!(destructive_label("send"));
    assert!(destructive_label("delete draft"));
    assert!(!destructive_label("new message"));
    let liked = Candidate {
        name: Some("Liked Songs".to_owned()),
        ..Candidate::default()
    };
    assert!(exact_named_match("open Liked Songs now", Some(&liked)));
    assert!(!exact_named_match("open Disliked Songs", Some(&liked)));
    assert!(!exact_named_match("open it", None));
    let single = Candidate {
        name: Some("Play".to_owned()),
        ..Candidate::default()
    };
    assert!(
        !exact_named_match("press Play", Some(&single)),
        "one word is not identity"
    );
    let described = target_payload(&Candidate {
        ref_id: "@s:e1".to_owned(),
        role: "button".to_owned(),
        description: Some("described".to_owned()),
        ..Candidate::default()
    });
    assert_eq!(described.name.as_deref(), Some("described"));
}

#[test]
fn a_stop_before_phrase_names_a_control_the_denylist_does_not_cover() {
    let phrases = vec!["discard the draft".to_owned()];
    assert!(named_in_stop_before("Discard", &phrases));
    assert!(named_in_stop_before("discard", &phrases));
    assert!(!named_in_stop_before("Reply", &phrases));
    // Too short to mean anything on its own; must never match by accident.
    assert!(!named_in_stop_before("Go", &phrases));
    assert!(!named_in_stop_before("Reply", &[]));
}

#[test]
fn is_destructive_covers_the_denylist_stop_before_phrases_and_unnamed_sheet_buttons() {
    let mut screen = clickable_screen();
    let discard = Candidate {
        name: Some("Discard".to_owned()),
        ..Candidate::default()
    };
    // Neither on the denylist nor named by any stop_before phrase.
    assert!(!is_destructive(&discard, &screen, &[]));
    // The flow's own words name it, even though the denylist does not.
    assert!(is_destructive(
        &discard,
        &screen,
        &["discard the draft".to_owned()]
    ));
    // The denylist alone is still enough, with no stop_before phrases at all.
    let send = Candidate {
        name: Some("Send".to_owned()),
        ..Candidate::default()
    };
    assert!(is_destructive(&send, &screen, &[]));
    // An unnamed button is only gated inside a confirmation sheet.
    let unnamed = Candidate::default();
    assert!(!is_destructive(&unnamed, &screen, &[]));
    screen.surface = "sheet".to_owned();
    assert!(is_destructive(&unnamed, &screen, &[]));
    assert!(
        !is_destructive(&discard, &screen, &[]),
        "a named, non-denylisted control in a sheet is still safe"
    );
}
