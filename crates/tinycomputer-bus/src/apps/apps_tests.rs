//! Unit tests pinning the application and window payloads' wire forms.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    CloseAppRequest, FocusWindowRequest, LaunchRequest, ListAppsRequest, ListSurfacesRequest,
    ListWindowsRequest, MoveWindowRequest, ResizeWindowRequest, WindowRequest,
};
use serde_json::json;

#[test]
fn a_launch_request_leaves_attach_if_running_unset_rather_than_guessing() {
    // Absent means "the engine's default", which is `true`. Encoding `false`
    // here would silently change what a bare launch does.
    let request = LaunchRequest::new("Safari");

    assert!(request.attach_if_running.is_none());
    assert!(request.args.is_empty());
    assert!(request.env.is_empty());
    assert!(!request.activate);
}

#[test]
fn a_launch_request_round_trips_its_environment_in_a_stable_order() {
    let mut request = LaunchRequest::new("Chromium");
    request.env.insert("B".to_owned(), "2".to_owned());
    request.env.insert("A".to_owned(), "1".to_owned());
    request.args.push("--headless".to_owned());
    request.cdp_port = Some(0);

    let value = serde_json::to_value(&request).expect("it serializes");
    // A `BTreeMap` so two identical requests encode identically.
    assert_eq!(value["env"], json!({ "A": "1", "B": "2" }));
    assert_eq!(value["cdp_port"], json!(0));

    let decoded: LaunchRequest = serde_json::from_value(value).expect("it decodes");
    assert_eq!(decoded, request);
}

#[test]
fn a_close_app_request_does_not_force_by_default() {
    let request: CloseAppRequest =
        serde_json::from_value(json!({ "app": "Slack" })).expect("it decodes");

    assert!(!request.force);
}

#[test]
fn the_listing_requests_accept_an_empty_object() {
    let apps: ListAppsRequest = serde_json::from_value(json!({})).expect("it decodes");
    let windows: ListWindowsRequest = serde_json::from_value(json!({})).expect("it decodes");
    let surfaces: ListSurfacesRequest = serde_json::from_value(json!({})).expect("it decodes");

    assert_eq!(apps, ListAppsRequest::default());
    assert_eq!(windows, ListWindowsRequest::default());
    assert_eq!(surfaces, ListSurfacesRequest::default());
}

#[test]
fn a_window_request_addresses_the_focused_window_when_empty() {
    let request: WindowRequest = serde_json::from_value(json!({})).expect("it decodes");

    assert!(request.app.is_none() && request.window_id.is_none());
}

#[test]
fn a_focus_window_request_carries_all_three_narrowing_fields() {
    let request: FocusWindowRequest =
        serde_json::from_value(json!({ "app": "Safari", "title": "Inbox" })).expect("it decodes");

    assert_eq!(request.title.as_deref(), Some("Inbox"));
    assert!(request.window_id.is_none());
}

#[test]
fn the_geometry_requests_carry_their_numbers_as_floats() {
    let resize = serde_json::to_value(ResizeWindowRequest {
        width: 1_280.0,
        height: 720.0,
        ..ResizeWindowRequest::default()
    })
    .expect("it serializes");
    let moved = serde_json::to_value(MoveWindowRequest {
        x: -100.0,
        y: 0.0,
        ..MoveWindowRequest::default()
    })
    .expect("it serializes");

    assert_eq!(resize["width"], json!(1_280.0));
    // A negative origin is legal: a window may sit on a display left of the
    // main one.
    assert_eq!(moved["x"], json!(-100.0));
}
