//! Unit tests pinning the wait payload's wire form.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::WaitRequest;
use serde_json::json;

#[test]
fn a_sleep_sets_only_the_millisecond_mode() {
    let request = WaitRequest::sleep(250);

    assert_eq!(request.ms, Some(250));
    assert!(request.element.is_none());
    assert!(request.event.is_none());
    assert!(request.timeout_ms.is_none());
}

#[test]
fn an_element_wait_carries_its_predicate() {
    let request: WaitRequest = serde_json::from_value(json!({
        "element": "@s1:e2",
        "predicate": "enabled",
        "timeout_ms": 5_000,
    }))
    .expect("it decodes");

    assert_eq!(request.element.as_deref(), Some("@s1:e2"));
    assert_eq!(request.predicate.as_deref(), Some("enabled"));
    assert_eq!(request.timeout_ms, Some(5_000));
}

#[test]
fn a_surface_wait_is_named_by_string_the_way_the_engine_names_it() {
    let request: WaitRequest =
        serde_json::from_value(json!({ "surface": "menu_closed" })).expect("it decodes");

    assert_eq!(request.surface.as_deref(), Some("menu_closed"));
}

#[test]
fn an_empty_wait_decodes_and_is_left_for_the_engine_to_reject() {
    // Selecting no mode is an `INVALID_ARGS` reply, not a decode failure, so
    // the caller gets a message naming the modes rather than a parse error.
    let request: WaitRequest = serde_json::from_value(json!({})).expect("it decodes");

    assert_eq!(request, WaitRequest::default());
}
