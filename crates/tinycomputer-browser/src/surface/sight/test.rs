//! Tests for reading sight's reply: controls, text, and when the tree is
//! read instead.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::{is_seen, screen, script, selector};

/// A booking widget as sight reads it: a cookie dialog in front, a field
/// named by the words above it, a city row a page marked `combobox`, and an
/// icon button.
fn reading() -> serde_json::Value {
    json!({
        "ok": true,
        "title": "Book a flight",
        "surface": "sheet",
        "unreachable": 0,
        "nodes": [
            {"text": "We use cookies", "path": ["dialog \"cookieconsent\""]},
            {"id": "1", "role": "button", "name": "Accept All", "description": "",
             "value": "", "states": [], "box": [10, 700, 120, 40],
             "path": ["dialog \"cookieconsent\""]},
            {"text": "To", "path": ["main"]},
            {"id": "2", "role": "textbox", "name": "To", "description": "Destination",
             "value": "Srin", "states": ["required"], "box": [10, 100, 300, 40],
             "path": ["main", "form"]},
            {"id": "3", "role": "button", "name": "Mumbai BOM", "description": "",
             "value": "", "states": ["covered"], "box": [10, 150, 300, 40],
             "path": ["main", "form"]},
            {"id": "4", "role": "checkbox", "name": "Return trip", "description": "",
             "value": "", "states": ["checked"], "box": [10, 200, 20, 20], "path": []},
            {"id": "5", "role": "button", "name": "close", "description": "an icon",
             "value": "", "states": ["offscreen"], "box": [], "path": []},
            {"text": "We use cookies", "path": ["main"]}
        ]
    })
}

#[test]
fn a_reading_becomes_controls_text_and_context_in_page_order() {
    let screen = screen(&reading()).unwrap();
    assert_eq!(screen.window.as_deref(), Some("Book a flight"));
    assert_eq!(screen.surface, "sheet");
    let refs = screen
        .candidates
        .iter()
        .map(|node| node.ref_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(refs, ["seen:1", "seen:2", "seen:3", "seen:4", "seen:5"]);
    assert_eq!(screen.context, ["We use cookies", "To"], "repeats kept once");
    assert_eq!(screen.text_nodes.len(), 3);
    assert_eq!(screen.text_nodes[0].role, "text");
    assert_eq!(screen.text_nodes[1].order, 2, "text keeps its place among controls");

    let field = &screen.candidates[1];
    assert_eq!(field.name.as_deref(), Some("To"));
    assert_eq!(field.description.as_deref(), Some("Destination"));
    assert_eq!(field.value, Some(json!("Srin")));
    assert_eq!(field.states, ["required"]);
    assert_eq!(field.available_actions, ["Click", "SetValue"]);
    assert_eq!(field.path, ["main", "form"]);
    assert_eq!(field.order, 3);
    assert_eq!(
        field.bounds,
        Some(json!({"x": 10.0, "y": 100.0, "width": 300.0, "height": 40.0}))
    );

    let row = &screen.candidates[2];
    assert_eq!(row.available_actions, ["Click"], "a row takes no text");
    assert!(row.description.is_none() && row.value.is_none());
    assert_eq!(screen.candidates[3].available_actions, ["Click", "Check"]);
    assert!(screen.candidates[4].bounds.is_none());
}

#[test]
fn a_reading_that_failed_or_saw_what_it_cannot_reach_gives_way_to_the_tree() {
    let mut unreachable = reading();
    unreachable["unreachable"] = json!(1);
    assert!(screen(&unreachable).is_none());
    assert!(screen(&json!({"ok": false, "reason": "root not found"})).is_none());
    assert!(screen(&json!(42)).is_none());
    assert!(screen(&json!({"ok": true})).is_none(), "no nodes");

    let bare = screen(&json!({"ok": true, "nodes": []})).unwrap();
    assert_eq!(bare.surface, "window");
    assert!(bare.window.is_none());
}

#[test]
fn a_seen_ref_is_addressed_by_its_mark_and_a_tree_ref_by_itself() {
    assert!(is_seen("seen:12"));
    assert!(!is_seen("e12"));
    assert_eq!(selector("seen:12"), r#"[data-tc-seen="12"]"#);
    assert_eq!(selector("e12"), "@e12");
    assert_eq!(selector("@e12"), "@e12");
}

#[test]
fn the_script_is_called_with_its_root_and_limits() {
    let whole = script(None);
    assert!(whole.starts_with('('), "{}", &whole[..40]);
    assert!(whole.contains("data-tc-seen"));
    assert!(whole.ends_with(
        r#"(null, {"controls":800,"labels":3000,"name":120,"text":160,"texts":400})"#
    ));
    let under = script(Some("seen:7"));
    assert!(under.contains(r#"("[data-tc-seen=\"7\"]", {"#), "{}", &under[under.len() - 120..]);
}
