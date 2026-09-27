//! Tests for the browser surface: snapshot parsing, and every surface call
//! turning into the right engine command over a scripted engine.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use serde_json::json;
use tinydesktop_bus::JevOperation;
use tinydesktop_bus::browser::SessionOptions;
use tinydesktop_core::Platform;
use tinydesktop_core::surface::{Candidate, Depth, Surface};
use tinydesktop_input::MotionProfile;

use super::tree::{parse_line, screen};
use super::{BrowserSurface, browser_key};
use crate::fake::{Fake, failure, ok};
use crate::sessions::Browser;

const PAGE: &str = r#"- banner
  - heading "Search flights" [level=1]
  - text: Fares include taxes
- main
  - textbox "From" [required, ref=e1]: Delhi
  - textbox "To" [ref=e2]
  - checkbox "Return trip" [checked=false, ref=e3]
  - checkbox "Flexible dates" [checked=true, ref=e4]
  - button "Search" [ref=e5]
  - button "Book" [disabled, ref=e6]
  - combobox "Cabin" [expanded=true, ref=e7]
    - option "Economy" [selected, ref=e8]
  - textbox "Notes" [ref=e9]
    - text: private note
"#;

#[test]
fn a_line_parses_its_role_name_attributes_and_value() {
    let line = parse_line(r#"    - textbox "Say \"hi\"" [required, ref=e1]: typed"#).unwrap();
    assert_eq!(line.depth, 2);
    assert_eq!(line.role, "textbox");
    assert_eq!(line.name.as_deref(), Some("Say \"hi\""));
    assert_eq!(
        line.attributes,
        [
            ("required".to_owned(), None),
            ("ref".to_owned(), Some("e1".to_owned()))
        ]
    );
    assert_eq!(line.value.as_deref(), Some("typed"));

    let text = parse_line("- text: Hello there").unwrap();
    assert_eq!(
        (text.role.as_str(), text.value.as_deref()),
        ("text", Some("Hello there"))
    );
    let bare = parse_line("- separator").unwrap();
    assert!(bare.name.is_none() && bare.value.is_none());
    assert_eq!(parse_line("- text:").unwrap().value, None);
    assert!(parse_line("not an item").is_none());
    assert!(parse_line(r#"- button "unterminated"#).is_none());
    assert!(parse_line(r"- button [ref=e1").is_none());
}

#[test]
fn a_page_becomes_candidates_and_context() {
    let parsed = screen(PAGE, "Flights");
    let names = parsed
        .candidates
        .iter()
        .map(|node| node.name.clone().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "From",
            "To",
            "Return trip",
            "Flexible dates",
            "Search",
            "Cabin",
            "Economy",
            "Notes"
        ],
        "the disabled Book button is not offered"
    );
    assert_eq!(parsed.window.as_deref(), Some("Flights"));
    assert_eq!(parsed.surface, "window");
    assert_eq!(parsed.context, ["Search flights", "Fares include taxes"]);
    assert!(
        parsed
            .text_nodes
            .iter()
            .any(|node| node.value.as_ref() == Some(&json!("private note"))),
        "field content stays reachable but out of context"
    );

    let from = &parsed.candidates[0];
    assert_eq!(from.ref_id, "e1");
    assert_eq!(from.value, Some(json!("Delhi")));
    assert_eq!(from.states, ["required"]);
    assert_eq!(from.available_actions, ["Click", "SetValue"]);
    assert_eq!(from.path, ["main"]);
    assert_eq!(parsed.candidates[2].available_actions, ["Click", "Check"]);
    assert!(parsed.candidates[2].states.is_empty());
    assert_eq!(parsed.candidates[3].states, ["checked"]);
    assert_eq!(parsed.candidates[4].available_actions, ["Click"]);
    assert_eq!(parsed.candidates[5].states, ["expanded"]);
    assert_eq!(parsed.candidates[6].states, ["selected"]);
    assert_eq!(parsed.candidates[6].path, ["main", "combobox \"Cabin\""]);
}

#[test]
fn dialogs_are_the_surface_in_front() {
    let dialog = screen("- dialog \"Sign in\"\n  - button \"Close\" [ref=e1]\n", "");
    assert_eq!(dialog.surface, "sheet");
    assert_eq!(dialog.window, None);
    let alert = screen("- dialog\n- alertdialog \"Leave?\"\n", "");
    assert_eq!(alert.surface, "alert");
    let many = (0..80)
        .map(|index| format!("- heading \"line {index}\"\n- text: \n"))
        .collect::<Vec<_>>()
        .concat();
    assert_eq!(screen(&many, "").context.len(), 60, "context is capped");
    let long = format!(
        "- heading \"{}\"\n- heading \"line 1\"\n- heading \"line 1\"\n",
        "x".repeat(400)
    );
    let capped = screen(&long, "");
    assert_eq!(capped.context[0].chars().count(), 160);
    assert_eq!(capped.context.len(), 2, "duplicates are kept once");
}

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

struct Harness {
    fake: Fake,
    surface: BrowserSurface,
    _runtime: tokio::runtime::Runtime,
}

fn harness(name: &str, fake: Fake) -> Harness {
    moving_harness(name, fake, MotionProfile::default())
}

fn moving_harness(name: &str, fake: Fake, motion: MotionProfile) -> Harness {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let browser = Browser::with_scratch(
        Arc::new(fake.clone()),
        std::env::temp_dir().join(format!(
            "tinydesktop-surface-test-{}-{name}",
            std::process::id()
        )),
    );
    let surface = BrowserSurface::new(
        Arc::new(browser),
        SessionOptions::default(),
        runtime.handle().clone(),
    )
    .with_motion(motion)
    .without_waiting();
    Harness {
        fake,
        surface,
        _runtime: runtime,
    }
}

fn page_fake() -> Fake {
    Fake::scripted(|command| match command["action"].as_str().unwrap() {
        "snapshot" => Some(ok(&json!({"snapshot": PAGE}))),
        "inputvalue" if command["selector"] == "@e1" => Some(ok(&json!({"value": "Delhi"}))),
        "inputvalue" => Some(ok(&json!({"value": ""}))),
        "gettext" if command["selector"] == "@e2" => Some(ok(&json!({"text": "Srinagar"}))),
        "gettext" => Some(ok(&json!({"text": " "}))),
        _ => None,
    })
}

fn node(reference: &str, actions: &[&str]) -> Candidate {
    Candidate {
        ref_id: reference.to_owned(),
        available_actions: actions.iter().map(|action| (*action).to_owned()).collect(),
        ..Candidate::default()
    }
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
    let Harness { fake, surface, .. } =
        moving_harness("execute", page_fake(), MotionProfile::Instant);
    assert_eq!(surface.motion(), MotionProfile::Instant);
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

/// A page whose result card lays a click layer over its own "Select"
/// button; `same_card` is what the page says about the covering element.
fn covered_fake(same_card: bool) -> Fake {
    Fake::scripted(move |command| match command["action"].as_str().unwrap() {
        "click" => Some(failure(
            "Element '@e5' is covered by <div.layer> at its click point, so the input would land on that element instead.",
        )),
        "boundingbox" => Some(ok(
            &json!({"x": 10.0, "y": 20.0, "width": 100.0, "height": 40.0}),
        )),
        "evaluate" => Some(ok(&json!({"result": same_card}))),
        _ => None,
    })
}

#[test]
fn a_click_covered_by_its_own_card_lands_on_the_card() {
    let Harness { fake, surface, .. } = harness("covered-card", covered_fake(true));
    let select = Candidate {
        name: Some("Select flight".to_owned()),
        ..node("e5", &["Click"])
    };
    let reply = surface.execute(JevOperation::Click, Some(select), None);
    assert!(reply.ok, "{:?}", reply.error);
    let script = fake.last("evaluate")["script"].as_str().unwrap().to_owned();
    assert!(script.ends_with(r#"(60, 40, "Select flight")"#), "{script}");
    let actions = fake.actions();
    let pressed = actions
        .iter()
        .position(|action| action == "mousedown")
        .unwrap();
    let released = actions
        .iter()
        .position(|action| action == "mouseup")
        .unwrap();
    assert!(pressed < released, "press, then release");
    assert!(
        actions[..pressed]
            .iter()
            .filter(|action| *action == "mousemove")
            .count()
            > 1,
        "the pointer travels to the card before pressing: {actions:?}"
    );
    let arrived = fake.last("mousemove");
    assert_eq!(
        (
            arrived["x"].as_f64(),
            arrived["y"].as_f64(),
            arrived["inputMode"].as_str()
        ),
        (Some(60.0), Some(40.0), Some("instant"))
    );
}

#[test]
fn a_click_glides_the_virtual_pointer_onto_its_target_first() {
    let fake = Fake::scripted(|command| {
        (command["action"] == "boundingbox")
            .then(|| ok(&json!({"x": 400.0, "y": 300.0, "width": 120.0, "height": 32.0})))
    });
    let Harness { fake, surface, .. } = harness("glide", fake);
    let reply = surface.execute(JevOperation::Click, Some(node("e5", &["Click"])), None);
    assert!(reply.ok, "{:?}", reply.error);
    let actions = fake.actions();
    let clicked = actions.iter().position(|action| action == "click").unwrap();
    let moves = actions[..clicked]
        .iter()
        .filter(|action| *action == "mousemove")
        .count();
    assert!(moves > 5, "{actions:?}");
    let arrived = fake.last("mousemove");
    let (x, y) = (
        arrived["x"].as_f64().unwrap(),
        arrived["y"].as_f64().unwrap(),
    );
    assert!((400.0..=520.0).contains(&x) && (300.0..=332.0).contains(&y));
    assert_eq!(fake.last("click")["selector"], "@e5");

    // The pointer stays where it ended: the next reach starts from there
    // rather than entering from somewhere new.
    let before = fake.sent().len();
    surface.execute(JevOperation::Check, Some(node("e5", &["Toggle"])), None);
    let next = fake.sent()[before..]
        .iter()
        .find(|command| command["action"] == "mousemove")
        .cloned()
        .unwrap();
    let step = (next["x"].as_f64().unwrap() - x).hypot(next["y"].as_f64().unwrap() - y);
    assert!(
        step < 60.0,
        "the second reach starts where the first ended: {step}"
    );
}

#[test]
fn an_element_without_a_box_is_clicked_without_a_glide() {
    let Harness { fake, surface, .. } = harness("no-box", page_fake());
    assert!(
        surface
            .execute(JevOperation::Click, Some(node("e5", &["Click"])), None)
            .ok
    );
    assert!(!fake.actions().iter().any(|action| action == "mousemove"));
    assert_eq!(fake.last("click")["selector"], "@e5");
}

#[test]
fn text_is_typed_key_by_key_into_a_clicked_and_selected_field() {
    let Harness { fake, surface, .. } = harness("typed", page_fake());
    let field = node("e2", &["SetValue"]);
    let reply = surface.execute(JevOperation::TypeText, Some(field), Some("SXR".to_owned()));
    assert!(reply.ok, "{:?}", reply.error);
    assert_eq!(fake.last("click")["selector"], "@e2");
    let select_all = fake.last("press");
    assert!(
        select_all["key"].as_str().unwrap().ends_with("+a"),
        "{select_all}"
    );
    let sent = fake.sent();
    let downs: String = sent
        .iter()
        .filter(|command| command["action"] == "keyboard" && command["eventType"] == "keyDown")
        .map(|command| command["text"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(downs, "SXR");
    let ups = sent
        .iter()
        .filter(|command| command["action"] == "keyboard" && command["eventType"] == "keyUp")
        .count();
    assert_eq!(ups, 3);
    assert!(!fake.actions().iter().any(|action| action == "fill"));

    let focused = surface.execute(JevOperation::TypeText, None, Some("a\nb".to_owned()));
    assert!(focused.ok);
    assert_eq!(fake.last("press")["key"], "Enter");
}

#[test]
fn a_field_that_cannot_be_clicked_or_a_long_text_is_filled_instead() {
    let refusing = Fake::scripted(|command| {
        (command["action"] == "click").then(|| failure("Element '@e2' is not visible"))
    });
    let Harness { fake, surface, .. } = harness("typed-refused", refusing);
    let reply = surface.execute(
        JevOperation::TypeText,
        Some(node("e2", &["SetValue"])),
        Some("SXR".to_owned()),
    );
    assert!(reply.ok, "{:?}", reply.error);
    assert_eq!(fake.last("fill")["value"], "SXR");

    let Harness { fake, surface, .. } = harness("typed-long", page_fake());
    let long = "x".repeat(401);
    surface.execute(
        JevOperation::TypeText,
        Some(node("e2", &["SetValue"])),
        Some(long.clone()),
    );
    assert_eq!(fake.last("fill")["value"], long.as_str());
    assert!(!fake.actions().iter().any(|action| action == "keyboard"));
}

#[test]
fn a_click_covered_by_anything_else_stays_refused() {
    let Harness { fake, surface, .. } = harness("covered-banner", covered_fake(false));
    let select = Candidate {
        name: Some("Select flight".to_owned()),
        ..node("e5", &["Click"])
    };
    let reply = surface.execute(JevOperation::Click, Some(select), None);
    assert!(!reply.ok);
    assert!(reply.error.unwrap().message.contains("is covered by"));
    assert!(!fake.actions().iter().any(|action| action == "mouse"));

    let Harness { fake, surface, .. } = harness("covered-unnamed", covered_fake(true));
    assert!(
        !surface
            .execute(JevOperation::Click, Some(node("e5", &["Click"])), None)
            .ok
    );
    assert!(!fake.actions().iter().any(|action| action == "evaluate"));
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
    let actions = fake.actions();
    let tail = &actions[actions.len() - 9..];
    assert_eq!(
        tail.iter().step_by(3).collect::<Vec<_>>(),
        ["focus", "press", "inserttext"]
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
    let unselectable = Fake::scripted(|command| {
        (command["action"] == "press").then(|| failure("Operation timed out"))
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
fn repeated_containers_are_numbered_so_their_cards_group() {
    let page = "- list \"Results\"\n  - listitem\n    - text: IndiGo\n    - text: ₹6,840\n    - button \"Select\" [ref=e1]\n  - listitem\n    - text: Vistara\n    - text: ₹7,210\n    - button \"Select\" [ref=e2]\n- list \"More\"\n  - listitem\n    - button \"Next page\" [ref=e3]\n";
    let parsed = screen(page, "Results");
    assert_eq!(
        parsed.candidates[0].path,
        ["list \"Results\"", "listitem #1"]
    );
    assert_eq!(
        parsed.candidates[1].path,
        ["list \"Results\"", "listitem #2"]
    );
    assert_eq!(
        parsed.candidates[2].path,
        ["list \"More\"", "listitem #1"],
        "numbering restarts under a new parent"
    );
    let groups = tinydesktop_core::surface::result_groups(&parsed);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].fields, ["IndiGo", "₹6,840", "Select"]);
    assert_eq!(groups[1].primary.as_ref().unwrap().ref_id, "e2");
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
