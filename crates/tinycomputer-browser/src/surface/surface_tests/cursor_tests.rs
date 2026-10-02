//! Tests for the screen cursor gliding onto a target before the surface acts.

use std::sync::Arc;

use serde_json::json;
use tinycomputer_bus::JevOperation;
use tinycomputer_bus::browser::SessionOptions;
use tinycomputer_core::surface::Surface;
use tinycomputer_cursor::{CursorPace, ScreenCursor};

use super::{Drawn, Harness, harness, node, page_fake, shown_harness};
use crate::fake::{Fake, failure, ok};
use crate::surface::cursor::viewport_origin;

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
    assert_eq!(drawn.glides(), [] as [std::vec::Vec<[f64; 3]>; 0]);
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
    assert_eq!(drawn.glides(), [] as [std::vec::Vec<[f64; 3]>; 0]);
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
    assert_eq!(drawn.glides(), [] as [std::vec::Vec<[f64; 3]>; 0]);
}
