//! Unit tests pinning the pasteboard payloads' wire forms.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{ClipboardGetRequest, ClipboardSetRequest};
use crate::vocabulary::ClipboardFormat;
use serde_json::json;

#[test]
fn a_clipboard_read_defaults_to_leaving_the_format_to_the_engine() {
    let request: ClipboardGetRequest = serde_json::from_value(json!({})).expect("it decodes");

    assert!(request.format.is_none());
    assert!(request.out.is_none());
}

#[test]
fn a_clipboard_read_names_its_format_in_snake_case() {
    let value = serde_json::to_value(ClipboardGetRequest {
        format: Some(ClipboardFormat::FileUrls),
        out: None,
    })
    .expect("it serializes");

    assert_eq!(value["format"], json!("file_urls"));
}

#[test]
fn a_text_clipboard_write_leaves_the_other_two_carriers_empty() {
    let request = ClipboardSetRequest::text("hello");

    assert_eq!(request.text.as_deref(), Some("hello"));
    assert!(request.image.is_none());
    assert!(request.file_urls.is_empty());
}

#[test]
fn a_clipboard_write_round_trips_file_urls() {
    let request = ClipboardSetRequest {
        file_urls: vec!["file:///tmp/a.txt".to_owned()],
        ..ClipboardSetRequest::default()
    };

    let decoded: ClipboardSetRequest =
        serde_json::from_value(serde_json::to_value(&request).expect("it serializes"))
            .expect("it decodes");

    assert_eq!(decoded, request);
}
