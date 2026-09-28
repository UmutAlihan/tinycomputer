//! Tests for navigation, snapshots, extraction, evaluation, and screenshots.

use serde_json::json;
use tinycomputer_bus::browser::{
    EvaluateRequest, ImageFormat, NavigateRequest, ReadFormat, ReadRequest, ScreenshotRequest,
    SnapshotRequest, Target, WaitUntil,
};

use super::invalid;
use crate::convert::{evaluate, navigate, read, screenshot, snapshot};

#[test]
fn navigation_maps_every_wait_condition() {
    for (wait_until, expected) in [
        (WaitUntil::Commit, "none"),
        (WaitUntil::DomContentLoaded, "domcontentloaded"),
        (WaitUntil::Load, "load"),
        (WaitUntil::NetworkIdle, "networkidle"),
    ] {
        let command = navigate(&NavigateRequest {
            url: "https://example.com".to_owned(),
            wait_until,
            timeout_ms: Some(5_000),
        })
        .unwrap();
        assert_eq!(command["waitUntil"], expected);
        assert_eq!(command["timeout"], 5_000);
    }
    let bare = navigate(&NavigateRequest::new("https://example.com")).unwrap();
    assert!(bare.get("timeout").is_none());
    assert!(invalid(navigate(&NavigateRequest::new("  "))).contains("url"));
}

#[test]
fn a_snapshot_carries_only_the_options_that_were_set() {
    assert_eq!(
        snapshot(&SnapshotRequest::interactive()),
        json!({"action": "snapshot", "interactive": true, "compact": false, "urls": false})
    );
    let scoped = snapshot(&SnapshotRequest {
        selector: Some("main".to_owned()),
        depth: Some(3),
        compact: true,
        include_urls: true,
        ..SnapshotRequest::default()
    });
    assert_eq!(scoped["maxDepth"], 3);
    assert_eq!(scoped["selector"], "main");
    assert_eq!(scoped["urls"], true);
}

#[test]
fn reading_picks_the_command_for_each_format() {
    let read_as = |format, selector: Option<&str>| {
        read(&ReadRequest {
            format,
            selector: selector.map(str::to_owned),
            ..ReadRequest::default()
        })
    };
    assert_eq!(
        read_as(ReadFormat::Markdown, None),
        json!({"action": "read"})
    );
    assert_eq!(
        read_as(ReadFormat::Html, None),
        json!({"action": "content"})
    );
    assert_eq!(
        read_as(ReadFormat::Html, Some("main")),
        json!({"action": "innerhtml", "selector": "main"})
    );
    assert_eq!(
        read_as(ReadFormat::Text, Some("main")),
        json!({"action": "gettext", "selector": "main"})
    );
    assert_eq!(
        read_as(ReadFormat::Markdown, Some("main"))["action"],
        "gettext"
    );
    assert_eq!(read_as(ReadFormat::Text, None)["selector"], "body");
}

#[test]
fn evaluation_and_screenshots_validate_their_input() {
    assert_eq!(
        evaluate(&EvaluateRequest::new("document.title")).unwrap(),
        json!({"action": "evaluate", "script": "document.title"})
    );
    assert!(invalid(evaluate(&EvaluateRequest::new(""))).contains("expression"));

    let shot = screenshot(
        &ScreenshotRequest {
            target: Some(Target::reference("e4")),
            full_page: true,
            format: ImageFormat::Jpeg,
            quality: Some(80),
        },
        "/tmp/shot.jpeg",
    )
    .unwrap();
    assert_eq!(
        shot,
        json!({"action": "screenshot", "path": "/tmp/shot.jpeg", "fullPage": true, "format": "jpeg", "quality": 80, "selector": "@e4"})
    );
    let webp = screenshot(
        &ScreenshotRequest {
            format: ImageFormat::Webp,
            ..ScreenshotRequest::default()
        },
        "p",
    )
    .unwrap();
    assert_eq!(webp["format"], "webp");
    assert_eq!(
        screenshot(&ScreenshotRequest::default(), "p").unwrap()["format"],
        "png"
    );
    assert!(
        invalid(screenshot(
            &ScreenshotRequest {
                quality: Some(0),
                ..ScreenshotRequest::default()
            },
            "p"
        ))
        .contains("quality")
    );
}
