//! Tests for the session payload types.
//!
//! The assertions on field names are the wire form: a host and a module that
//! disagree about `default_timeout_ms` fail at runtime with a decode error, and
//! nothing else in the build would catch it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{SessionId, SessionInfo, SessionOptions, Viewport};
use serde_json::json;

#[test]
fn session_id_is_a_bare_string_on_the_wire() {
    let encoded = serde_json::to_value(SessionId::new("s-7")).expect("serializes");
    assert_eq!(encoded, json!("s-7"));

    let decoded: SessionId = serde_json::from_value(json!("s-7")).expect("deserializes");
    assert_eq!(decoded.as_str(), "s-7");
}

#[test]
fn session_id_displays_as_its_identity() {
    assert_eq!(SessionId::from("s-7").to_string(), "s-7");
}

#[test]
fn default_options_are_a_headless_desktop_browser() {
    let options = SessionOptions::default();

    assert!(options.headless);
    assert_eq!(options.viewport, Viewport::desktop(1280, 800));
    assert_eq!(options.default_timeout_ms, 30_000);
    assert!(options.endpoint.is_none());
    assert!(options.download_dir.is_none());
    assert!(options.allowed_origins.is_empty());
}

#[test]
fn options_fill_every_absent_field_from_the_default() {
    // `#[serde(default)]` on the struct is what lets a host send `{}` and a
    // later contract version add a field without breaking it.
    let options: SessionOptions = serde_json::from_value(json!({})).expect("deserializes");
    assert_eq!(options, SessionOptions::default());

    let partial: SessionOptions =
        serde_json::from_value(json!({ "headless": false })).expect("deserializes");
    assert!(!partial.headless);
    assert_eq!(partial.default_timeout_ms, 30_000);
}

#[test]
fn options_serialize_with_the_documented_field_names() {
    let encoded = serde_json::to_value(SessionOptions::default()).expect("serializes");
    let object = encoded.as_object().expect("an object");

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "allowed_origins",
            "args",
            "default_timeout_ms",
            "download_dir",
            "endpoint",
            "executable",
            "headless",
            "user_agent",
            "user_data_dir",
            "viewport",
        ]
    );
}

#[test]
fn viewport_round_trips() {
    let viewport = Viewport {
        width: 390,
        height: 844,
        device_scale_factor: 3.0,
        mobile: true,
    };
    let encoded = serde_json::to_value(viewport).expect("serializes");

    assert_eq!(
        encoded,
        json!({
            "width": 390,
            "height": 844,
            "device_scale_factor": 3.0,
            "mobile": true,
        })
    );
    assert_eq!(
        serde_json::from_value::<Viewport>(encoded).expect("deserializes"),
        viewport
    );
}

#[test]
fn session_info_round_trips() {
    let info = SessionInfo {
        id: SessionId::new("s-1"),
        endpoint: "ws://127.0.0.1:9222/devtools/browser/abc".to_string(),
        launched: true,
        headless: true,
        viewport: Viewport::default(),
        url: "https://example.com/".to_string(),
        title: "Example Domain".to_string(),
    };

    let encoded = serde_json::to_value(&info).expect("serializes");
    assert_eq!(encoded["id"], json!("s-1"));
    assert_eq!(
        serde_json::from_value::<SessionInfo>(encoded).expect("deserializes"),
        info
    );
}

#[test]
fn a_session_id_is_reachable_from_both_string_forms() {
    assert_eq!(SessionId::from("s-1".to_string()), SessionId::new("s-1"));
    assert_eq!(SessionId::from("s-1"), SessionId::new("s-1"));
}

#[test]
fn session_ids_order_and_hash_so_a_host_can_key_by_them() {
    let mut ids = vec![SessionId::new("s-2"), SessionId::new("s-1")];
    ids.sort();

    assert_eq!(ids, vec![SessionId::new("s-1"), SessionId::new("s-2")]);
    assert_eq!(
        std::collections::HashSet::from([SessionId::new("s-1"), SessionId::new("s-1")]).len(),
        1
    );
}

#[test]
fn a_viewport_can_be_built_for_a_phone() {
    let phone = Viewport {
        mobile: true,
        device_scale_factor: 3.0,
        ..Viewport::desktop(390, 844)
    };

    assert!(phone.mobile);
    assert_ne!(phone, Viewport::default());
}
