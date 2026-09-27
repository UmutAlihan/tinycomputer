//! Tests for the snapshot payload types.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{ElementRef, Snapshot, SnapshotRequest};
use serde_json::json;

#[test]
fn snapshot_defaults_to_the_whole_bounded_tree() {
    let request = SnapshotRequest::default();

    assert!(!request.interactive_only);
    assert!(request.selector.is_none());
    assert_eq!(request.max_chars, 200_000);
    assert_eq!(
        serde_json::from_value::<SnapshotRequest>(json!({})).expect("deserializes"),
        request
    );
}

#[test]
fn interactive_keeps_every_other_default() {
    let request = SnapshotRequest::interactive();

    assert!(request.interactive_only);
    assert_eq!(request.max_chars, SnapshotRequest::default().max_chars);
}

#[test]
fn request_serializes_with_the_documented_field_names() {
    let encoded = serde_json::to_value(SnapshotRequest::default()).expect("serializes");
    let mut keys: Vec<&str> = encoded
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();

    assert_eq!(
        keys,
        [
            "compact",
            "depth",
            "include_urls",
            "interactive_only",
            "max_chars",
            "selector",
        ]
    );
}

#[test]
fn snapshot_round_trips() {
    let snapshot = Snapshot {
        url: "https://example.com/".to_string(),
        title: "Example Domain".to_string(),
        sequence: 1,
        tree: "- document \"Example Domain\"\n  - link \"More information\" @e1".to_string(),
        refs: vec![ElementRef {
            id: "e1".to_string(),
            role: "link".to_string(),
            name: "More information".to_string(),
        }],
        truncated: false,
    };

    let encoded = serde_json::to_value(&snapshot).expect("serializes");
    assert_eq!(encoded["refs"][0]["id"], json!("e1"));
    assert_eq!(
        serde_json::from_value::<Snapshot>(encoded).expect("deserializes"),
        snapshot
    );
}

#[test]
fn refs_are_named_without_their_marker() {
    // The `@` belongs to the rendered tree, not to the identity: `Target::parse`
    // strips it, and a ref that carried it would round-trip to `@@e1`.
    let element = ElementRef {
        id: "e1".to_string(),
        role: "link".to_string(),
        name: "More information".to_string(),
    };

    assert!(!element.id.starts_with('@'));
}
