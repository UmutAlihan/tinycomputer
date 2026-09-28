//! Tests for the browser surface: snapshot parsing, and every surface call
//! turning into the right engine command over a scripted engine.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use serde_json::json;
use tinycomputer_bus::JevOperation;
use tinycomputer_bus::browser::SessionOptions;
use tinycomputer_core::Platform;
use tinycomputer_core::surface::{Candidate, Depth, Surface};
use tinycomputer_cursor::{CursorPace, OverlayCommand, OverlaySink, ScreenCursor};

use super::cursor::viewport_origin;
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
    shown_harness(name, fake, SessionOptions::default(), &Drawn::default())
}

fn shown_harness(name: &str, fake: Fake, options: SessionOptions, drawn: &Drawn) -> Harness {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let browser = Browser::with_scratch(
        Arc::new(fake.clone()),
        std::env::temp_dir().join(format!(
            "tinycomputer-surface-test-{}-{name}",
            std::process::id()
        )),
    );
    let surface = BrowserSurface::new(Arc::new(browser), options, runtime.handle().clone())
        .with_cursor(drawn.cursor(CursorPace::Natural));
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
        // A text field is focused, so targetless typing is accepted.
        "evaluate" => Some(ok(&json!({"result": true}))),
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
    let mouse = fake
        .actions()
        .iter()
        .filter(|action| *action == "mouse")
        .count();
    assert_eq!(mouse, 3, "move, press, release");
    let released = fake.last("mouse");
    assert_eq!(
        (
            released["eventType"].as_str(),
            released["x"].as_f64(),
            released["y"].as_f64()
        ),
        (Some("mouseReleased"), Some(60.0), Some(40.0))
    );
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
    let groups = tinycomputer_core::surface::result_groups(&parsed);
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

/// Records every command the shared cursor would draw.
#[derive(Clone, Default)]
struct Drawn(Arc<std::sync::Mutex<Vec<OverlayCommand>>>);

impl OverlaySink for Drawn {
    fn send(&mut self, command: &OverlayCommand) -> std::io::Result<()> {
        self.0.lock().unwrap().push(command.clone());
        Ok(())
    }
}

impl Drawn {
    fn cursor(&self, pace: CursorPace) -> Arc<ScreenCursor> {
        Arc::new(ScreenCursor::with_sink(pace, Box::new(self.clone())).without_waiting())
    }

    /// The `[t, x, y]` path of every glide drawn so far.
    fn glides(&self) -> Vec<Vec<[f64; 3]>> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|command| match command {
                OverlayCommand::Glide { path, .. } => Some(path.clone()),
                OverlayCommand::Hide => None,
            })
            .collect()
    }
}

fn headed() -> SessionOptions {
    SessionOptions {
        headless: false,
        ..SessionOptions::default()
    }
}

/// A page whose `e5` sits at a known box in a window at (100, 50) with 80
/// points of toolbars above the page; `evaluate` fails when `refuse`.
fn boxed_fake(refuse: bool) -> Fake {
    Fake::scripted(move |command| match command["action"].as_str().unwrap() {
        "boundingbox" => Some(ok(
            &json!({"x": 400.0, "y": 300.0, "width": 120.0, "height": 32.0}),
        )),
        "evaluate" if refuse => Some(failure("Evaluation failed: CSP")),
        // The focused element takes text, so a fill goes ahead.
        "evaluate"
            if command["script"]
                .as_str()
                .unwrap()
                .contains("activeElement") =>
        {
            Some(ok(&json!({"result": true})))
        }
        "evaluate" => Some(ok(
            &json!({"result": [100.0, 50.0, 1280.0, 880.0, 1280.0, 800.0]}),
        )),
        _ => None,
    })
}

#[test]
fn the_viewport_is_placed_below_the_toolbars_and_inside_the_borders() {
    assert_eq!(
        viewport_origin(&[100.0, 50.0, 1296.0, 888.0, 1280.0, 800.0]),
        Some((108.0, 130.0))
    );
    assert_eq!(
        viewport_origin(&[0.0, 0.0, 800.0, 600.0, 900.0, 700.0]),
        Some((0.0, 0.0))
    );
    assert_eq!(viewport_origin(&[1.0, 2.0]), None);
}

#[test]
fn a_visible_session_glides_the_screen_cursor_onto_the_target_before_acting() {
    let drawn = Drawn::default();
    let Harness { fake, surface, .. } =
        shown_harness("cursor-headed", boxed_fake(false), headed(), &drawn);
    assert_eq!(surface.cursor().pace(), CursorPace::Natural);
    let reply = surface.execute(JevOperation::Click, Some(node("e5", &["Click"])), None);
    assert!(reply.ok, "{:?}", reply.error);

    let actions = fake.actions();
    let located = actions
        .iter()
        .position(|action| action == "evaluate")
        .unwrap();
    let clicked = actions.iter().position(|action| action == "click").unwrap();
    assert!(
        located < clicked,
        "the cursor lands before the click: {actions:?}"
    );
    assert_eq!(
        fake.last("click")["selector"],
        "@e5",
        "the click itself is unchanged"
    );
    assert!(
        !actions.iter().any(|action| action.starts_with("mouse")),
        "no input is sent"
    );

    let glides = drawn.glides();
    let [_, x, y] = *glides[0].last().unwrap();
    // The box, moved into screen points by the window's position and toolbars.
    assert!(
        (500.0..=620.0).contains(&x) && (430.0..=462.0).contains(&y),
        "{x}, {y}"
    );

    surface.execute(
        JevOperation::TypeText,
        Some(node("e5", &["SetValue"])),
        Some("SXR".to_owned()),
    );
    let next = &drawn.glides()[1];
    assert_eq!(next[0].map(f64::to_bits), [0.0, x, y].map(f64::to_bits));
    assert_eq!(fake.last("fill")["value"], "SXR");
}

#[test]
fn a_headless_session_or_an_off_cursor_draws_nothing() {
    let drawn = Drawn::default();
    let Harness { fake, surface, .. } = shown_harness(
        "cursor-headless",
        boxed_fake(false),
        SessionOptions::default(),
        &drawn,
    );
    assert!(
        surface
            .execute(JevOperation::Click, Some(node("e5", &["Click"])), None)
            .ok
    );
    assert!(drawn.glides().is_empty());
    assert!(!fake.actions().iter().any(|action| action == "boundingbox"));

    let Harness { fake, surface, .. } = harness("cursor-default", boxed_fake(false));
    let surface = surface.with_cursor(Arc::new(ScreenCursor::off()));
    assert!(
        surface
            .execute(JevOperation::Click, Some(node("e5", &["Click"])), None)
            .ok
    );
    assert!(!fake.actions().iter().any(|action| action == "boundingbox"));
}

#[test]
fn an_attached_browser_is_visible_even_when_marked_headless() {
    let attached = SessionOptions {
        endpoint: Some("ws://127.0.0.1:9222/devtools/browser/x".to_owned()),
        ..SessionOptions::default()
    };
    let drawn = Drawn::default();
    let Harness { surface, .. } =
        shown_harness("cursor-attached", boxed_fake(false), attached, &drawn);
    surface.execute(JevOperation::Check, Some(node("e5", &["Toggle"])), None);
    assert_eq!(drawn.glides().len(), 1);
}

#[test]
fn operations_without_a_pointer_or_a_box_draw_nothing() {
    let drawn = Drawn::default();
    let Harness { fake, surface, .. } =
        shown_harness("cursor-no-box", page_fake(), headed(), &drawn);
    assert!(
        surface
            .execute(JevOperation::Click, Some(node("e5", &["Click"])), None)
            .ok
    );
    assert!(drawn.glides().is_empty(), "no box, no cursor");
    assert_eq!(fake.last("click")["selector"], "@e5");

    let Harness { surface, .. } =
        shown_harness("cursor-scroll", boxed_fake(false), headed(), &drawn);
    surface.execute(JevOperation::Scroll, Some(node("e5", &["Scroll"])), None);
    surface.execute(JevOperation::TypeText, None, Some("x".to_owned()));
    assert!(drawn.glides().is_empty());
}

#[test]
fn a_page_that_will_not_say_where_its_window_is_still_gets_its_action() {
    let drawn = Drawn::default();
    let Harness { fake, surface, .. } =
        shown_harness("cursor-refused", boxed_fake(true), headed(), &drawn);
    assert!(
        surface
            .execute(JevOperation::Click, Some(node("e5", &["Click"])), None)
            .ok
    );
    assert_eq!(fake.last("click")["selector"], "@e5");
    assert!(drawn.glides().is_empty());
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

#[test]
fn an_unnamed_control_is_named_by_what_it_shows_but_a_field_never_is() {
    // A list of cities whose rows carry a `combobox` role and an
    // `aria-labelledby` that points nowhere: no accessible name at all.
    let tree = r#"- main
  - combobox [ref=e10]
    - generic
      - text: Mumbai
      - text: Chhatrapati Shivaji Maharaj International Airport
    - text: BOM
  - combobox [ref=e11]
    - text: Srinagar
  - textbox [ref=e12]
    - text: what was typed
  - combobox [ref=e13]: typed value
    - text: suggestion
  - button "Search" [ref=e14]
    - text: Search
"#;
    let parsed = screen(tree, "Flights");
    let described = |reference: &str| {
        parsed
            .candidates
            .iter()
            .find(|candidate| candidate.ref_id == reference)
            .unwrap()
            .description
            .clone()
    };
    assert_eq!(
        described("e10").as_deref(),
        Some("Mumbai Chhatrapati Shivaji Maharaj International Airport BOM")
    );
    assert_eq!(described("e11").as_deref(), Some("Srinagar"));
    assert_eq!(
        described("e12"),
        None,
        "a text field's content stays private"
    );
    assert_eq!(
        described("e13"),
        None,
        "a control holding a value is a field"
    );
    assert_eq!(described("e14"), None, "a named control keeps its name");
}
