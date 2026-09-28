//! Unit tests pinning the synthesized-input payloads' wire forms.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    DragEndpoint, DragRequest, HoldKeyRequest, HoldMouseRequest, HoverRequest, MouseClickRequest,
    MouseMoveRequest, MouseWheelRequest, PressRequest,
};
use crate::vocabulary::{Modifier, MouseButton};
use serde_json::json;

#[test]
fn a_press_request_defaults_to_the_focused_application() {
    let request: PressRequest =
        serde_json::from_value(json!({ "combo": "cmd+shift+p" })).expect("it decodes");

    assert_eq!(request, PressRequest::new("cmd+shift+p"));
    assert!(request.app.is_none());
    assert!(!request.force);
}

#[test]
fn a_hold_key_request_carries_the_same_shape_as_a_press() {
    // The two reserved members validate a payload before failing closed, so
    // the payload has to decode even though the call never succeeds.
    let request: HoldKeyRequest =
        serde_json::from_value(json!({ "combo": "shift", "force": true })).expect("it decodes");

    assert_eq!(request.combo, "shift");
    assert!(request.force);
}

#[test]
fn a_hover_request_accepts_either_a_ref_or_a_point() {
    let by_ref: HoverRequest =
        serde_json::from_value(json!({ "ref_id": "@s1:e2" })).expect("it decodes");
    let by_point: HoverRequest =
        serde_json::from_value(json!({ "x": 10.0, "y": 20.0 })).expect("it decodes");

    assert_eq!(by_ref.ref_id.as_deref(), Some("@s1:e2"));
    assert!(by_ref.x.is_none());
    assert_eq!(by_point.y, Some(20.0));
    assert!(by_point.ref_id.is_none());
}

#[test]
fn a_drag_endpoint_is_a_ref_or_a_point_and_says_which() {
    assert_eq!(DragEndpoint::at_ref("@s1:e2").x, None);
    assert_eq!(DragEndpoint::at_point(1.0, 2.0).ref_id, None);
}

#[test]
fn a_drag_request_round_trips_both_endpoints() {
    let request = DragRequest {
        from: DragEndpoint::at_ref("@s1:e2"),
        to: DragEndpoint::at_point(400.0, 300.0),
        drop_delay_ms: Some(250),
        ..DragRequest::default()
    };

    let decoded: DragRequest =
        serde_json::from_value(serde_json::to_value(&request).expect("it serializes"))
            .expect("it decodes");

    assert_eq!(decoded, request);
}

#[test]
fn a_mouse_move_request_is_two_coordinates() {
    let value = serde_json::to_value(MouseMoveRequest { x: 5.0, y: 6.0 }).expect("it serializes");

    assert_eq!(value, json!({ "x": 5.0, "y": 6.0 }));
}

#[test]
fn a_mouse_click_request_defaults_to_the_left_button_and_no_modifiers() {
    let request: MouseClickRequest =
        serde_json::from_value(json!({ "x": 1.0, "y": 2.0 })).expect("it decodes");

    assert_eq!(request.button, MouseButton::Left);
    assert_eq!(request.count, 0);
    assert!(request.modifiers.is_empty());
}

#[test]
fn a_mouse_click_request_carries_its_modifiers_in_order() {
    let request = MouseClickRequest {
        x: 1.0,
        y: 2.0,
        button: MouseButton::Right,
        count: 2,
        modifiers: vec![Modifier::Meta, Modifier::Shift],
    };

    let value = serde_json::to_value(&request).expect("it serializes");
    assert_eq!(value["modifiers"], json!(["Meta", "Shift"]));
    assert_eq!(value["button"], json!("Right"));
}

#[test]
fn a_hold_mouse_request_carries_a_button_and_a_point() {
    let request: HoldMouseRequest =
        serde_json::from_value(json!({ "x": 1.0, "y": 2.0, "button": "Middle" }))
            .expect("it decodes");

    assert_eq!(request.button, MouseButton::Middle);
}

#[test]
fn a_mouse_wheel_request_separates_the_two_axes() {
    let request: MouseWheelRequest =
        serde_json::from_value(json!({ "x": 1.0, "y": 2.0, "dy": -3.0 })).expect("it decodes");

    // Exact comparison is right here: these are the caller's own literals
    // carried through JSON, not the result of arithmetic that could round.
    assert_eq!(serde_json::to_value(request.dy).unwrap(), json!(-3.0));
    assert_eq!(serde_json::to_value(request.dx).unwrap(), json!(0.0));
}
