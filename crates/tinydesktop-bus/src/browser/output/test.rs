//! Tests for the screenshot and held-output payload types.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{ImageFormat, OutputChunk, OutputId, OutputRef, ScreenshotRequest};
use serde_json::json;

#[test]
fn output_id_is_a_bare_string_on_the_wire() {
    assert_eq!(
        serde_json::to_value(OutputId::new("o-3")).expect("serializes"),
        json!("o-3")
    );
    assert_eq!(OutputId::from("o-3").to_string(), "o-3");
}

#[test]
fn screenshots_default_to_a_png_of_the_viewport() {
    let request = ScreenshotRequest::default();

    assert_eq!(request.format, ImageFormat::Png);
    assert!(!request.full_page);
    assert!(request.target.is_none());
    assert_eq!(
        serde_json::from_value::<ScreenshotRequest>(json!({})).expect("deserializes"),
        request
    );
}

#[test]
fn image_formats_carry_their_media_type() {
    assert_eq!(ImageFormat::Png.media_type(), "image/png");
    assert_eq!(ImageFormat::Jpeg.media_type(), "image/jpeg");
    assert_eq!(ImageFormat::Webp.media_type(), "image/webp");
}

#[test]
fn image_format_is_snake_case_on_the_wire() {
    assert_eq!(
        serde_json::to_value(ImageFormat::Jpeg).expect("serializes"),
        json!("jpeg")
    );
}

#[test]
fn output_ref_round_trips() {
    let handle = OutputRef {
        id: OutputId::new("o-1"),
        total_bytes: 4_096,
        sha256: "0".repeat(64),
        media_type: ImageFormat::Png.media_type().to_string(),
        width: 1280,
        height: 800,
    };

    let encoded = serde_json::to_value(&handle).expect("serializes");
    assert_eq!(encoded["id"], json!("o-1"));
    assert_eq!(encoded["media_type"], json!("image/png"));
    assert_eq!(
        serde_json::from_value::<OutputRef>(encoded).expect("deserializes"),
        handle
    );
}

#[test]
fn chunks_carry_base64_and_say_where_they_end() {
    let chunk = OutputChunk {
        id: OutputId::new("o-1"),
        offset: 0,
        data: "aGk=".to_string(),
        eof: true,
    };

    let encoded = serde_json::to_value(&chunk).expect("serializes");
    assert_eq!(encoded["data"], json!("aGk="));
    assert_eq!(
        serde_json::from_value::<OutputChunk>(encoded).expect("deserializes"),
        chunk
    );
}

#[test]
fn an_output_id_is_reachable_from_both_string_forms() {
    // A host holds these as `String` after a decode and as `&str` from a
    // literal; both conversions exist so neither call site has to know.
    assert_eq!(OutputId::from("o-1".to_string()), OutputId::new("o-1"));
    assert_eq!(OutputId::from("o-1"), OutputId::new("o-1"));
    assert_eq!(OutputId::new("o-1").as_str(), "o-1");
}

#[test]
fn output_ids_order_and_hash_so_a_host_can_key_by_them() {
    let mut ids = vec![OutputId::new("o-2"), OutputId::new("o-1")];
    ids.sort();

    assert_eq!(ids, vec![OutputId::new("o-1"), OutputId::new("o-2")]);
    assert_eq!(
        std::collections::HashSet::from([OutputId::new("o-1"), OutputId::new("o-1")]).len(),
        1
    );
}

#[test]
fn a_screenshot_request_round_trips_every_field() {
    let request = ScreenshotRequest {
        target: Some(crate::Target::selector("#chart")),
        full_page: true,
        format: ImageFormat::Webp,
        quality: Some(60),
    };
    let encoded = serde_json::to_value(&request).expect("serializes");

    assert_eq!(encoded["format"], json!("webp"));
    assert_eq!(encoded["target"]["kind"], json!("selector"));
    assert_eq!(
        serde_json::from_value::<ScreenshotRequest>(encoded).expect("deserializes"),
        request
    );
}

#[test]
fn image_formats_deserialize_from_their_wire_spellings() {
    for (wire, format) in [
        ("png", ImageFormat::Png),
        ("jpeg", ImageFormat::Jpeg),
        ("webp", ImageFormat::Webp),
    ] {
        assert_eq!(
            serde_json::from_value::<ImageFormat>(json!(wire)).expect("deserializes"),
            format
        );
    }
}
