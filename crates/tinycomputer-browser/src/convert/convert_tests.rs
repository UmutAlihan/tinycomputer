//! Tests pinning each contract request to the agent-browser command it becomes.
//!
//! These are the wire form on the engine side: a field agent-browser does not
//! read is a silent no-op at runtime, so every name is asserted here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;
use tinycomputer_bus::browser::{
    Action, EvaluateRequest, ImageFormat, LocateBy, Locator, NavigateRequest, ReadFormat,
    ReadRequest, ScreenshotRequest, ScrollDirection, SessionOptions, SnapshotRequest, Target,
    Viewport, WaitState, WaitUntil,
};

use super::session::allowed_domains;
use super::{action, evaluate, launch, navigate, read, screenshot, snapshot, viewport};
use crate::error::Error;

fn invalid(result: crate::Result<serde_json::Value>) -> String {
    match result {
        Err(Error::InvalidInput { message }) => message,
        other => panic!("expected invalid input, got {other:?}"),
    }
}

#[test]
fn a_default_session_launches_headless_with_nothing_else() {
    assert_eq!(
        launch(&SessionOptions::default()),
        json!({"action": "launch", "headless": true, "args": []})
    );
}

#[test]
fn every_session_option_reaches_its_launch_field() {
    let options = SessionOptions {
        endpoint: Some("ws://127.0.0.1:9222".to_owned()),
        executable: Some("/usr/bin/chromium".to_owned()),
        headless: false,
        user_agent: Some("agent".to_owned()),
        user_data_dir: Some("/profiles/trip".to_owned()),
        download_dir: Some("/downloads".to_owned()),
        args: vec!["--lang=en".to_owned()],
        allowed_origins: vec!["https://example.com".to_owned()],
        viewport: Viewport::desktop(800, 600),
        ..SessionOptions::default()
    };
    assert_eq!(
        launch(&options),
        json!({
            "action": "launch",
            "headless": false,
            "args": ["--lang=en"],
            "cdpUrl": "ws://127.0.0.1:9222",
            "executablePath": "/usr/bin/chromium",
            "userAgent": "agent",
            "profile": "/profiles/trip",
            "downloadPath": "/downloads",
            "allowedDomains": ["example.com"],
        })
    );
    assert_eq!(
        viewport(&options),
        json!({"action": "viewport", "width": 800, "height": 600, "deviceScaleFactor": 1.0, "mobile": false})
    );
}

#[test]
fn origins_become_host_patterns_and_subdomain_wildcards() {
    let origins = [
        "https://Example.com",
        "https://.booking.com",
        "http://localhost:8080/path",
        "plain.host",
        "https://",
    ]
    .map(str::to_owned);
    assert_eq!(
        allowed_domains(&origins),
        ["example.com", "*.booking.com", "localhost", "plain.host"]
    );
}

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
fn refs_and_selectors_become_engine_selectors() {
    let click = action(
        &Action::Click {
            target: Target::parse("@e12"),
            new_tab: true,
        },
        1_000,
    )
    .unwrap();
    assert_eq!(
        click,
        json!({"action": "click", "selector": "@e12", "newTab": true})
    );
    let fill = action(
        &Action::Fill {
            target: Target::selector("#email"),
            value: "a@b.c".to_owned(),
        },
        1_000,
    )
    .unwrap();
    assert_eq!(
        fill,
        json!({"action": "fill", "selector": "#email", "value": "a@b.c"})
    );
}

#[test]
fn every_plain_action_names_its_engine_command() {
    let target = Target::reference("e1");
    let cases = [
        (
            Action::DoubleClick {
                target: target.clone(),
            },
            "dblclick",
        ),
        (
            Action::Hover {
                target: target.clone(),
            },
            "hover",
        ),
        (
            Action::Focus {
                target: target.clone(),
            },
            "focus",
        ),
        (
            Action::Select {
                target: target.clone(),
                values: vec!["SXR".to_owned()],
            },
            "select",
        ),
        (
            Action::Check {
                target: target.clone(),
                checked: true,
            },
            "check",
        ),
        (
            Action::Check {
                target: target.clone(),
                checked: false,
            },
            "uncheck",
        ),
        (
            Action::GetText {
                target: target.clone(),
            },
            "gettext",
        ),
        (
            Action::GetAttribute {
                target: target.clone(),
                attribute: "href".to_owned(),
            },
            "getattribute",
        ),
        (
            Action::IsVisible {
                target: target.clone(),
            },
            "isvisible",
        ),
        (Action::Back, "back"),
        (Action::Forward, "forward"),
        (Action::Reload, "reload"),
        (
            Action::Press {
                key: "Enter".to_owned(),
            },
            "press",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(
            action(&input, 1_000).unwrap()["action"],
            expected,
            "{input:?}"
        );
    }
    assert!(
        invalid(action(
            &Action::Press {
                key: " ".to_owned()
            },
            1_000
        ))
        .contains("key")
    );
}

#[test]
fn typing_with_and_without_a_target() {
    let focused = action(
        &Action::Type {
            target: None,
            text: "Srinagar".to_owned(),
            delay_ms: None,
        },
        1_000,
    )
    .unwrap();
    assert_eq!(focused, json!({"action": "inserttext", "text": "Srinagar"}));
    let targeted = action(
        &Action::Type {
            target: Some(Target::reference("e3")),
            text: "SXR".to_owned(),
            delay_ms: Some(40),
        },
        1_000,
    )
    .unwrap();
    assert_eq!(
        targeted,
        json!({"action": "type", "selector": "@e3", "text": "SXR", "delay": 40})
    );
}

#[test]
fn scrolling_maps_directions_and_the_page_ends() {
    let scroll = |direction, pixels, target| {
        action(
            &Action::Scroll {
                direction,
                pixels,
                target,
            },
            1_000,
        )
        .unwrap()
    };
    assert_eq!(
        scroll(ScrollDirection::Down, None, None),
        json!({"action": "scroll", "direction": "down", "amount": 300})
    );
    assert_eq!(scroll(ScrollDirection::Up, Some(50), None)["amount"], 50);
    assert_eq!(
        scroll(ScrollDirection::Left, None, None)["direction"],
        "left"
    );
    assert_eq!(
        scroll(ScrollDirection::Right, None, None)["direction"],
        "right"
    );
    assert_eq!(scroll(ScrollDirection::Top, None, None)["direction"], "up");
    let bottom = scroll(
        ScrollDirection::Bottom,
        None,
        Some(Target::selector(".results")),
    );
    assert_eq!(bottom["direction"], "down");
    assert_eq!(bottom["selector"], ".results");
}

#[test]
fn waiting_prefers_text_then_target_then_a_pause() {
    let wait = |target, text, ms, timeout_ms| {
        action(
            &Action::WaitFor {
                target,
                text,
                state: WaitState::Hidden,
                ms,
                timeout_ms,
            },
            25_000,
        )
    };
    assert_eq!(
        wait(None, Some("Results".to_owned()), None, None).unwrap(),
        json!({"action": "wait", "text": "Results", "timeout": 25_000})
    );
    assert_eq!(
        wait(Some(Target::reference("e2")), None, None, Some(10)).unwrap(),
        json!({"action": "wait", "selector": "@e2", "state": "hidden", "timeout": 10})
    );
    assert_eq!(
        wait(None, None, Some(250), None).unwrap(),
        json!({"action": "wait", "timeout": 250})
    );
    assert!(invalid(wait(None, None, None, None)).contains("wait_for"));
    for state in [WaitState::Attached, WaitState::Detached, WaitState::Visible] {
        let command = action(
            &Action::WaitFor {
                target: Some(Target::reference("e2")),
                text: None,
                state,
                ms: None,
                timeout_ms: None,
            },
            1,
        )
        .unwrap();
        assert_eq!(command["state"], serde_json::to_value(state).unwrap());
    }
}

#[test]
fn locators_use_the_semantic_commands_with_a_subaction() {
    let by = |by, value: &str| Locator::new(by, value);
    let click = action(
        &Action::Click {
            target: Target::locator(by(LocateBy::Role, "button").with_name("Search")),
            new_tab: false,
        },
        1,
    )
    .unwrap();
    assert_eq!(
        click,
        json!({"action": "getbyrole", "role": "button", "exact": false, "subaction": "click", "name": "Search"})
    );
    let fill = action(
        &Action::Fill {
            target: Target::locator(by(LocateBy::Label, "From")),
            value: "Delhi".to_owned(),
        },
        1,
    )
    .unwrap();
    assert_eq!(fill["action"], "getbylabel");
    assert_eq!(fill["value"], "Delhi");
    for (locate, command, key, subaction) in [
        (LocateBy::Text, "getbytext", "text", "hover"),
        (
            LocateBy::Placeholder,
            "getbyplaceholder",
            "placeholder",
            "text",
        ),
        (LocateBy::TestId, "getbytestid", "testId", "check"),
        (LocateBy::AltText, "getbyalttext", "text", "click"),
        (LocateBy::Title, "getbytitle", "text", "click"),
    ] {
        let target = Target::locator(by(locate, "x"));
        let input = match subaction {
            "hover" => Action::Hover { target },
            "text" => Action::GetText { target },
            "check" => Action::Check {
                target,
                checked: true,
            },
            _ => Action::Click {
                target,
                new_tab: false,
            },
        };
        let built = action(&input, 1).unwrap();
        assert_eq!(built["action"], command);
        assert_eq!(built[key], "x");
        assert_eq!(built["subaction"], subaction);
    }
}

#[test]
fn a_locator_the_engine_cannot_combine_is_refused_with_the_alternative() {
    let locator = Target::locator(Locator::new(LocateBy::Text, "Book"));
    let message = invalid(action(
        &Action::Focus {
            target: locator.clone(),
        },
        1,
    ));
    assert!(message.contains("ref or selector"), "{message}");
    let unchecking = invalid(action(
        &Action::Check {
            target: locator,
            checked: false,
        },
        1,
    ));
    assert!(unchecking.contains("uncheck"), "{unchecking}");
    let indexed = Locator {
        index: 2,
        ..Locator::new(LocateBy::Text, "Book")
    };
    assert!(
        invalid(action(
            &Action::Click {
                target: Target::locator(indexed),
                new_tab: false
            },
            1
        ))
        .contains("index")
    );
    assert!(
        invalid(action(
            &Action::Click {
                target: Target::locator(Locator::new(LocateBy::Text, " ")),
                new_tab: false
            },
            1
        ))
        .contains("value")
    );
    assert!(
        invalid(action(
            &Action::Click {
                target: Target::selector(""),
                new_tab: false
            },
            1
        ))
        .contains("non-empty")
    );
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
