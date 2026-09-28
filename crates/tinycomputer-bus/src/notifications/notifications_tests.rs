//! Unit tests pinning the notification payloads' wire forms.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    DismissAllNotificationsRequest, DismissNotificationRequest, ListNotificationsRequest,
    NotificationActionRequest,
};
use serde_json::json;

#[test]
fn a_notification_listing_accepts_an_empty_object() {
    let request: ListNotificationsRequest = serde_json::from_value(json!({})).expect("it decodes");

    assert_eq!(request, ListNotificationsRequest::default());
}

#[test]
fn a_notification_action_carries_the_identity_the_caller_expects_to_find() {
    let request: NotificationActionRequest = serde_json::from_value(json!({
        "index": 0,
        "action": "Reply",
        "expected_app": "Messages",
        "expected_title": "Ada",
    }))
    .expect("it decodes");

    assert_eq!(request.expected_app.as_deref(), Some("Messages"));
    assert_eq!(request.expected_title.as_deref(), Some("Ada"));
}

#[test]
fn an_expected_identity_is_optional_on_the_wire_so_its_absence_is_reportable() {
    // The engine requires both. Making them mandatory here would turn a
    // missing field into a decode failure the caller cannot act on, instead of
    // the `INVALID_ARGS` reply that names what is missing.
    let request: NotificationActionRequest =
        serde_json::from_value(json!({ "index": 0, "action": "Reply" })).expect("it decodes");

    assert!(request.expected_app.is_none());
}

#[test]
fn a_dismiss_request_separates_the_filter_from_the_expected_identity() {
    let request: DismissNotificationRequest = serde_json::from_value(json!({
        "index": 2,
        "app": "Mail",
        "expected_app": "Mail",
        "expected_title": "Digest",
    }))
    .expect("it decodes");

    // `app` narrows the list before indexing; `expected_app` verifies what was
    // found. They are the same value here and need not be.
    assert_eq!(request.index, 2);
    assert_eq!(request.app.as_deref(), Some("Mail"));
}

#[test]
fn a_dismiss_all_request_defaults_to_every_application() {
    let request: DismissAllNotificationsRequest =
        serde_json::from_value(json!({})).expect("it decodes");

    assert!(request.app.is_none());
}
