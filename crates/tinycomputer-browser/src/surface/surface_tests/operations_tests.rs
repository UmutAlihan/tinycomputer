//! Tests for every surface call turning into the right engine command over a
//! scripted engine.

use serde_json::json;
use tinycomputer_bus::JevOperation;
use tinycomputer_core::Platform;
use tinycomputer_core::surface::{Depth, Surface};

use super::{Harness, PAGE, harness, node, page_fake};
use crate::fake::{Fake, failure, ok};
use crate::surface::operations::browser_key;

#[test]
fn keys_are_spelled_the_way_agent_browser_reads_them() {
    assert_eq!(browser_key("cmd+a", Platform::MacOs), "Meta+a");
    assert_eq!(browser_key("cmd+a", Platform::Linux), "Control+a");
    assert_eq!(
        browser_key("ctrl+shift+Z", Platform::MacOs),
        "Control+Shift+z"
    );
    assert_eq!(browser_key("alt+left", Platform::Windows), "Alt+ArrowLeft");
    assert_eq!(browser_key("option+Down", Platform::MacOs), "Alt+ArrowDown");
    for (combo, key) in [
        ("return", "Enter"),
        ("esc", "Escape"),
        ("tab", "Tab"),
        ("space", "Space"),
        ("backspace", "Backspace"),
        ("delete", "Delete"),
        ("f5", "F5"),
        ("meta+command+k", "Meta+Meta+k"),
    ] {
        assert_eq!(browser_key(combo, Platform::MacOs), key);
    }
    assert_eq!(browser_key(" + ", Platform::MacOs), "");
}

#[test]
fn observing_opens_the_session_once_and_reads_the_page() {
    let Harness { fake, surface, .. } = harness("observe", page_fake());
    assert!(surface.session().is_none());
    let screen = surface.observe("flights", None, Depth::Skeleton).unwrap();
    assert_eq!(screen.app, "flights");
    assert_eq!(screen.window.as_deref(), Some("Flights"));
    assert_eq!(fake.last("snapshot")["maxDepth"], 6);
    let scoped = surface.observe("", Some("@e7"), Depth::Skeleton).unwrap();
    assert_eq!(scoped.app, "browser");
    let sent = fake.last("snapshot");
    assert_eq!(sent["selector"], "@e7");
    assert!(sent.get("maxDepth").is_none());
    assert_eq!(
        fake.actions()
            .iter()
            .filter(|action| *action == "launch")
            .count(),
        1
    );
    assert!(surface.session().is_some());
    assert!(format!("{surface:?}").contains("BrowserSurface"));
}

#[test]
fn a_failed_observation_is_an_envelope_with_the_wire_code() {
    let fake = Fake::scripted(|command| {
        (command["action"] == "snapshot").then(|| failure("Unknown ref: e9"))
    });
    let Harness { surface, .. } = harness("observe-failed", fake);
    let error = surface.observe("", Some("e9"), Depth::Full).unwrap_err();
    assert_eq!(error.error.unwrap().code, "STALE_REF");

    let unlaunchable = Fake::scripted(|command| {
        (command["action"] == "launch").then(|| failure("Auto-launch failed: no chrome"))
    });
    let Harness { surface, .. } = harness("unlaunchable", unlaunchable);
    assert_eq!(
        surface.launch("browser").error.unwrap().code,
        "BROWSER_UNAVAILABLE"
    );
}

#[test]
fn every_operation_becomes_its_engine_command() {
    let Harness { fake, surface, .. } = harness("execute", page_fake());
    let button = node("e5", &["Click"]);
    for (operation, action) in [
        (JevOperation::Click, "click"),
        (JevOperation::Expand, "click"),
        (JevOperation::Collapse, "click"),
        (JevOperation::TypeText, "fill"),
        (JevOperation::Check, "check"),
        (JevOperation::Uncheck, "uncheck"),
        (JevOperation::Scroll, "scroll"),
        (JevOperation::Wait, "wait"),
    ] {
        let reply = surface.execute(operation, Some(button.clone()), Some("SXR".to_owned()));
        assert!(reply.ok, "{operation:?}");
        assert_eq!(
            fake.actions().iter().rev().nth(2).unwrap(),
            action,
            "{operation:?}"
        );
    }
    assert_eq!(fake.last("fill")["value"], "SXR");
    assert_eq!(fake.last("click")["selector"], "@e5");
    for operation in [
        JevOperation::Drill,
        JevOperation::Widen,
        JevOperation::Done,
        JevOperation::Blocked,
    ] {
        assert!(surface.execute(operation, None, None).ok);
    }
    let untargeted = surface.execute(JevOperation::Click, Some(node("", &[])), None);
    assert_eq!(untargeted.error.unwrap().code, "INVALID_TARGET");
    let focused = surface.execute(JevOperation::TypeText, None, Some("Srinagar".to_owned()));
    assert!(
        focused.ok,
        "text without a target goes to the focused element"
    );
    assert_eq!(
        fake.last("inserttext"),
        json!({"action": "inserttext", "text": "Srinagar"})
    );
    assert!(surface.execute(JevOperation::Scroll, None, None).ok);
}

#[test]
fn typing_without_a_target_refuses_when_nothing_editable_is_focused() {
    // No `evaluate` script here: `default_reply` answers `{"result": 42}`,
    // which is not `true`, so the focused element is read as not editable.
    let fake = Fake::scripted(|command| {
        (command["action"] == "snapshot").then(|| ok(&json!({"snapshot": PAGE})))
    });
    let Harness { fake, surface, .. } = harness("focus-not-editable", fake);
    let refused = surface.execute(JevOperation::TypeText, None, Some("secret".to_owned()));
    assert_eq!(refused.error.unwrap().code, "INVALID_TARGET");
    assert!(
        !fake.actions().iter().any(|action| action == "inserttext"),
        "an unverified focus must never receive the text"
    );
}

#[test]
fn values_are_read_from_the_field_then_its_text() {
    let Harness { surface, .. } = harness("read", page_fake());
    assert_eq!(
        surface.read_value(&node("e1", &[])).as_deref(),
        Some("Delhi")
    );
    assert_eq!(
        surface.read_value(&node("e2", &[])).as_deref(),
        Some("Srinagar")
    );
    assert_eq!(surface.read_value(&node("e3", &[])), None);
    assert_eq!(surface.read_value(&node("", &[])), None);
}

#[test]
fn pasting_focuses_selects_and_inserts_without_a_clipboard() {
    let Harness { fake, surface, .. } = harness("paste", page_fake());
    assert!(
        surface
            .paste("", &node("e2", &["Click", "SetValue"]), "Srinagar")
            .ok
    );
    let actions = fake
        .actions()
        .into_iter()
        .filter(|action| ["focus", "evaluate", "press", "inserttext"].contains(&action.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        &actions[actions.len() - 4..],
        ["focus", "evaluate", "press", "inserttext"],
        "focus, check it takes text, select, insert"
    );
    assert_eq!(fake.last("inserttext")["text"], "Srinagar");
    let before = fake.actions().len();
    assert!(surface.paste("", &node("e9", &["Click"]), "note").ok);
    assert!(
        !fake.actions()[before..]
            .iter()
            .any(|action| action == "press"),
        "a field that cannot be set is typed into at the caret"
    );
    assert_eq!(
        surface.paste("", &node("", &[]), "x").error.unwrap().code,
        "INVALID_TARGET"
    );
}

#[test]
fn a_paste_stops_at_the_first_failed_step() {
    let unfocusable = Fake::scripted(|command| {
        (command["action"] == "focus").then(|| failure("Element not found: @e2"))
    });
    let Harness { surface, .. } = harness("paste-focus", unfocusable);
    assert_eq!(
        surface
            .paste("", &node("e2", &["SetValue"]), "x")
            .error
            .unwrap()
            .code,
        "NO_SUCH_ELEMENT"
    );
    let unselectable = Fake::scripted(|command| match command["action"].as_str().unwrap() {
        "press" => Some(failure("Operation timed out")),
        "evaluate" => Some(ok(&json!({"result": true}))),
        _ => None,
    });
    let Harness { surface, .. } = harness("paste-select", unselectable);
    assert_eq!(
        surface
            .paste("", &node("e2", &["SetValue"]), "x")
            .error
            .unwrap()
            .code,
        "TIMEOUT"
    );
}

#[test]
fn pressing_launching_settling_and_navigating() {
    let Harness { fake, surface, .. } = harness("misc", page_fake());
    assert!(surface.press("", "return").ok);
    assert_eq!(fake.last("press")["key"], "Enter");
    assert_eq!(surface.press("", "").error.unwrap().code, "INVALID_KEY");
    let launched = surface.launch("browser");
    assert_eq!(launched.data.unwrap()["running"], true);
    surface.settle();
    let idle = fake.last("waitforloadstate");
    assert_eq!(
        (idle["state"].as_str(), idle["timeout"].as_u64()),
        (Some("networkidle"), Some(2_000))
    );
    assert_eq!(fake.last("wait")["timeout"], 400);
    let loaded = surface.navigate("https://flights.test/search");
    assert_eq!(loaded.data.unwrap()["url"], "https://flights.test/search");
    let back = surface.back("browser");
    assert!(back.ok, "{:?}", back.error);
    assert_eq!(fake.last("back")["action"], "back");
    assert!(back.data.unwrap().get("url").is_some());
    let refused = Fake::scripted(|command| {
        (command["action"] == "navigate")
            .then(|| failure("Domain 'evil.test' is not in the allowed domains list"))
    });
    let Harness { surface, .. } = harness("refused", refused);
    assert_eq!(
        surface.navigate("https://evil.test").error.unwrap().code,
        "BLOCKED_BY_POLICY"
    );
}

#[test]
fn closing_ends_the_session_and_is_harmless_twice() {
    let Harness {
        fake,
        surface,
        _runtime: runtime,
    } = harness("close", page_fake());
    surface.close();
    assert!(surface.launch("browser").ok);
    assert!(surface.session().is_some());
    surface.close();
    assert!(surface.session().is_none());
    // The close runs on the runtime; wait for it to land.
    runtime.block_on(async {
        for _ in 0..100 {
            if fake.actions().iter().any(|action| action == "close") {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("the session was never closed");
    });
}

#[test]
fn text_is_never_filled_or_pasted_into_an_element_that_does_not_take_it() {
    // A `div` with a `combobox` role — a city in a list of suggestions —
    // focuses, but what takes focus is no input: the page's check says no.
    let rows = Fake::scripted(|command| match command["action"].as_str().unwrap() {
        "evaluate" => Some(ok(&json!({"result": false}))),
        _ => None,
    });
    let Harness { fake, surface, .. } = harness("not-a-field", rows);
    let row = node("e216", &["Click", "SetValue"]);
    let filled = surface.execute(
        JevOperation::TypeText,
        Some(row.clone()),
        Some("Srinagar".to_owned()),
    );
    assert_eq!(filled.error.unwrap().code, "NOT_A_TEXT_FIELD");
    let pasted = surface.paste("", &row, "Srinagar");
    assert_eq!(pasted.error.unwrap().code, "NOT_A_TEXT_FIELD");
    let actions = fake.actions();
    assert!(
        !actions
            .iter()
            .any(|action| action == "fill" || action == "inserttext" || action == "press"),
        "nothing is typed: {actions:?}"
    );
    assert_eq!(fake.last("focus")["selector"], "@e216");
}
