//! Tests for the navigation and extraction payload types.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{EvaluateRequest, NavigateRequest, PageState, PageText, ReadFormat, ReadRequest};
use crate::WaitUntil;
use serde_json::json;

#[test]
fn wait_until_is_snake_case_on_the_wire() {
    assert_eq!(
        serde_json::to_value(WaitUntil::DomContentLoaded).expect("serializes"),
        json!("dom_content_loaded")
    );
    assert_eq!(
        serde_json::to_value(WaitUntil::NetworkIdle).expect("serializes"),
        json!("network_idle")
    );
    assert_eq!(
        serde_json::from_value::<WaitUntil>(json!("commit")).expect("deserializes"),
        WaitUntil::Commit
    );
}

#[test]
fn navigate_defaults_to_waiting_for_load() {
    let request: NavigateRequest =
        serde_json::from_value(json!({ "url": "https://example.com" })).expect("deserializes");

    assert_eq!(request.wait_until, WaitUntil::Load);
    assert!(request.timeout_ms.is_none());
    assert_eq!(request, NavigateRequest::new("https://example.com"));
}

#[test]
fn navigate_round_trips_an_explicit_deadline() {
    let request = NavigateRequest {
        url: "https://example.com/login".to_string(),
        wait_until: WaitUntil::NetworkIdle,
        timeout_ms: Some(5_000),
    };
    let encoded = serde_json::to_value(&request).expect("serializes");

    assert_eq!(
        encoded,
        json!({
            "url": "https://example.com/login",
            "wait_until": "network_idle",
            "timeout_ms": 5_000,
        })
    );
    assert_eq!(
        serde_json::from_value::<NavigateRequest>(encoded).expect("deserializes"),
        request
    );
}

#[test]
fn page_state_carries_an_absent_status_for_non_http_pages() {
    let state = PageState::new("about:blank");

    assert!(state.status.is_none());
    assert_eq!(
        serde_json::to_value(&state).expect("serializes"),
        json!({ "url": "about:blank", "title": "", "status": null })
    );
}

#[test]
fn read_defaults_to_bounded_markdown() {
    let request = ReadRequest::default();

    assert_eq!(request.format, ReadFormat::Markdown);
    assert_eq!(request.max_chars, 200_000);
    assert_eq!(
        serde_json::from_value::<ReadRequest>(json!({})).expect("deserializes"),
        request
    );
}

#[test]
fn read_format_is_snake_case_on_the_wire() {
    assert_eq!(
        serde_json::to_value(ReadFormat::Html).expect("serializes"),
        json!("html")
    );
}

#[test]
fn page_text_reports_truncation_explicitly() {
    let text = PageText {
        url: "https://example.com/".to_string(),
        title: "Example Domain".to_string(),
        format: ReadFormat::Text,
        content: "Example".to_string(),
        truncated: true,
    };
    let encoded = serde_json::to_value(&text).expect("serializes");

    assert_eq!(encoded["truncated"], json!(true));
    assert_eq!(
        serde_json::from_value::<PageText>(encoded).expect("deserializes"),
        text
    );
}

#[test]
fn evaluate_awaits_promises_by_default() {
    let request = EvaluateRequest::new("fetch('/api').then((r) => r.status)");

    assert!(request.await_promise);
    assert_eq!(
        serde_json::from_value::<EvaluateRequest>(json!({
            "expression": "fetch('/api').then((r) => r.status)"
        }))
        .expect("deserializes"),
        request
    );
}
