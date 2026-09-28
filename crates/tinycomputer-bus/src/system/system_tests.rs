//! Unit tests pinning the permissions payload's wire form.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::PermissionsRequest;
use serde_json::json;

#[test]
fn reporting_permissions_does_not_prompt_by_default() {
    let request: PermissionsRequest = serde_json::from_value(json!({})).expect("it decodes");

    assert!(!request.request);
}

#[test]
fn prompting_has_to_be_asked_for_explicitly() {
    let request: PermissionsRequest =
        serde_json::from_value(json!({ "request": true })).expect("it decodes");

    assert!(request.request);
    assert_eq!(
        serde_json::to_value(request).expect("it serializes"),
        json!({ "request": true })
    );
}
