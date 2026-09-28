//! Unit tests pinning the ref-action payloads' wire forms.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{RefRequest, ScrollRequest, SelectRequest, SetValueRequest, TypeRequest};
use crate::vocabulary::Direction;
use serde_json::json;

#[test]
fn a_bare_ref_request_needs_only_the_ref() {
    let request: RefRequest =
        serde_json::from_value(json!({ "ref_id": "@s1:e2" })).expect("it decodes");

    assert_eq!(request, RefRequest::new("@s1:e2"));
    assert!(request.snapshot_id.is_none());
    assert!(request.timeout_ms.is_none());
}

#[test]
fn a_ref_request_serializes_its_absent_fields_as_null() {
    // `#[serde(default)]` on the struct makes them optional inbound; they stay
    // present outbound so a reader can see the request was fully specified.
    let value = serde_json::to_value(RefRequest::new("@s1:e2")).expect("it serializes");

    assert_eq!(
        value,
        json!({ "ref_id": "@s1:e2", "snapshot_id": null, "timeout_ms": null })
    );
}

#[test]
fn a_type_request_carries_its_text_verbatim() {
    let request: TypeRequest =
        serde_json::from_value(json!({ "ref_id": "@s1:e2", "text": "  spaced  " }))
            .expect("it decodes");

    // No trimming in the contract: what the caller sent is what is typed.
    assert_eq!(request.text, "  spaced  ");
}

#[test]
fn set_value_and_select_share_a_shape_but_not_a_type() {
    let set_value: SetValueRequest =
        serde_json::from_value(json!({ "ref_id": "@s1:e2", "value": "42" })).expect("it decodes");
    let select: SelectRequest =
        serde_json::from_value(json!({ "ref_id": "@s1:e3", "value": "Monday" }))
            .expect("it decodes");

    assert_eq!(set_value.value, "42");
    assert_eq!(select.value, "Monday");
}

#[test]
fn a_scroll_request_omits_its_absent_optional_fields() {
    let value = serde_json::to_value(ScrollRequest::new("@s1:e4", Direction::Down, 3))
        .expect("it serializes");

    assert_eq!(
        value,
        json!({ "ref_id": "@s1:e4", "direction": "Down", "amount": 3 })
    );
}

#[test]
fn a_scroll_request_round_trips_with_every_field_set() {
    let request = ScrollRequest {
        snapshot_id: Some("s1".to_owned()),
        timeout_ms: Some(2_000),
        ..ScrollRequest::new("@s1:e4", Direction::Left, 10)
    };

    let decoded: ScrollRequest =
        serde_json::from_value(serde_json::to_value(&request).expect("it serializes"))
            .expect("it decodes");

    assert_eq!(decoded, request);
}
