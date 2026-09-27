//! Unit tests pinning the observation payloads' wire forms.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{FindRequest, GetRequest, IsRequest, ScreenshotRequest, SnapshotRequest};
use crate::vocabulary::{ElementProperty, ElementStateProperty, StatePredicate, Surface};
use serde_json::json;

#[test]
fn an_empty_object_decodes_as_a_default_snapshot() {
    let request: SnapshotRequest = serde_json::from_value(json!({})).expect("it decodes");

    assert_eq!(request, SnapshotRequest::default());
    assert_eq!(request.surface, Surface::Window);
    assert!(request.max_depth.is_none());
    assert!(!request.skeleton);
}

#[test]
fn a_snapshot_request_serializes_every_field_by_its_documented_name() {
    let value = serde_json::to_value(SnapshotRequest {
        app: Some("Safari".to_owned()),
        window_id: Some("w1".to_owned()),
        max_depth: Some(3),
        include_bounds: true,
        interactive_only: true,
        compact: true,
        surface: Surface::Menu,
        skeleton: true,
        root_ref: Some("@s1:e2".to_owned()),
        snapshot_id: Some("s1".to_owned()),
    })
    .expect("it serializes");

    assert_eq!(
        value,
        json!({
            "app": "Safari",
            "window_id": "w1",
            "max_depth": 3,
            "include_bounds": true,
            "interactive_only": true,
            "compact": true,
            "surface": "menu",
            "skeleton": true,
            "root_ref": "@s1:e2",
            "snapshot_id": "s1",
        })
    );
}

#[test]
fn a_partial_find_request_fills_the_rest_in_from_defaults() {
    let request: FindRequest =
        serde_json::from_value(json!({ "role": "button", "name": "Save" })).expect("it decodes");

    assert_eq!(request.role.as_deref(), Some("button"));
    assert!(!request.exact);
    assert!(request.states.is_empty());
    assert!(request.limit.is_none());
}

#[test]
fn find_state_predicates_survive_a_round_trip() {
    let request = FindRequest {
        role: Some("button".to_owned()),
        states: vec![
            StatePredicate::set("enabled"),
            StatePredicate::expect("focused", false),
        ],
        first: true,
        ..FindRequest::default()
    };

    let decoded: FindRequest =
        serde_json::from_value(serde_json::to_value(&request).expect("it serializes"))
            .expect("it decodes");

    assert_eq!(decoded, request);
}

#[test]
fn a_get_request_omits_an_absent_snapshot_id() {
    let value = serde_json::to_value(GetRequest::new("@s1:e2", ElementProperty::Value))
        .expect("it serializes");

    assert_eq!(value, json!({ "ref_id": "@s1:e2", "property": "value" }));
}

#[test]
fn an_is_request_omits_an_absent_snapshot_id() {
    let value = serde_json::to_value(IsRequest::new("@s1:e2", ElementStateProperty::Checked))
        .expect("it serializes");

    assert_eq!(value, json!({ "ref_id": "@s1:e2", "property": "checked" }));
}

#[test]
fn a_screenshot_request_carries_its_output_path_as_a_plain_string() {
    // A `PathBuf` would encode the same way here but would not decode from a
    // non-UTF-8 host, so the contract stays with `String` and the module crate
    // converts.
    let value = serde_json::to_value(ScreenshotRequest {
        output_path: Some("/tmp/shot.png".to_owned()),
        ..ScreenshotRequest::default()
    })
    .expect("it serializes");

    assert_eq!(value["output_path"], json!("/tmp/shot.png"));
}
