//! Serialization tests for the download wire contract.

#![allow(clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::{DownloadId, DownloadInfo, DownloadState, DownloadWaitRequest};

#[test]
fn ids_are_bare_strings_and_reachable_from_both_string_forms() {
    assert_eq!(
        serde_json::to_value(DownloadId::new("download-7")).expect("serializes"),
        json!("download-7")
    );
    assert_eq!(DownloadId::from("download-7").as_str(), "download-7");
    assert_eq!(
        DownloadId::from("download-7".to_owned()).to_string(),
        "download-7"
    );
}

#[test]
fn states_are_snake_case_and_terminal_only_when_finished() {
    assert_eq!(
        serde_json::to_value(DownloadState::InProgress).expect("serializes"),
        json!("in_progress")
    );
    assert!(!DownloadState::default().is_terminal());
    assert!(DownloadState::Completed.is_terminal());
    assert!(DownloadState::Cancelled.is_terminal());
}

#[test]
fn a_download_handle_round_trips_every_field() {
    let info = DownloadInfo {
        sequence: 3,
        id: DownloadId::new("guid"),
        url: "https://example.com/file.dmg".to_owned(),
        suggested_filename: "file.dmg".to_owned(),
        state: DownloadState::Completed,
        received_bytes: 42,
        total_bytes: Some(42),
        path: Some("/downloads/file.dmg".to_owned()),
    };
    let encoded = serde_json::to_value(&info).expect("serializes");
    assert_eq!(encoded["state"], "completed");
    assert_eq!(
        serde_json::from_value::<DownloadInfo>(encoded).expect("deserializes"),
        info
    );
}

#[test]
fn a_wait_request_defaults_to_the_session_deadline() {
    assert_eq!(
        serde_json::from_value::<DownloadWaitRequest>(json!({})).expect("deserializes"),
        DownloadWaitRequest::default()
    );
}
