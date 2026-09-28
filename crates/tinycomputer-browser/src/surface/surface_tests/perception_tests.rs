//! Tests for reading a page by sight, and falling back to the tree.

use serde_json::json;
use tinycomputer_bus::JevOperation;
use tinycomputer_core::surface::{Depth, Surface};

use super::{Harness, harness, page_fake};
use crate::fake::{Fake, failure, ok};
use crate::surface::{Denoised, Perception};

/// A page read by sight: one field and one result link a card covers.
fn sighted_fake() -> Fake {
    Fake::scripted(|command| match command["action"].as_str().unwrap() {
        "evaluate"
            if command["script"]
                .as_str()
                .unwrap()
                .contains("__tinycomputerSeen") =>
        {
            Some(ok(&json!({"result": {
                "ok": true,
                "title": "Flights",
                "surface": "window",
                "unreachable": 0,
                "denoised": {"ads": 2, "empty": 1, "hidden": 0},
                "nodes": [
                    {"id": "1", "role": "textbox", "name": "To", "states": [], "path": []},
                    {"id": "2", "role": "link", "name": "", "states": [], "path": []}
                ]
            }})))
        }
        "click" => Some(failure(
            "Element is covered by <div.layer> at its click point, so the input would land on that element instead.",
        )),
        "boundingbox" => Some(ok(
            &json!({"x": 10.0, "y": 20.0, "width": 100.0, "height": 40.0}),
        )),
        "evaluate" => Some(ok(&json!({"result": true}))),
        _ => None,
    })
}

#[test]
fn sight_reads_the_page_and_its_refs_reach_their_marks() {
    let Harness { fake, surface, .. } = harness("sight", sighted_fake());
    let screen = surface.observe("flights", None, Depth::Skeleton).unwrap();
    assert!(!fake.actions().iter().any(|action| action == "snapshot"));
    assert_eq!(screen.app, "flights");
    let field = screen.candidates[0].clone();
    assert_eq!(field.ref_id, "seen:1");

    let typed = surface.execute(
        JevOperation::TypeText,
        Some(field),
        Some("Srinagar".to_owned()),
    );
    assert!(typed.ok, "{:?}", typed.error);
    let fill = fake.last("fill");
    assert_eq!(fill["selector"], r#"[data-tc-seen="1"]"#, "{fill}");
    assert_eq!(fake.last("focus")["selector"], r#"[data-tc-seen="1"]"#);

    // An unnamed link a card covers is still clicked through its card: its
    // mark names it exactly.
    let link = screen.candidates[1].clone();
    let reply = surface.execute(JevOperation::Click, Some(link), None);
    assert!(reply.ok, "{:?}", reply.error);
    let script = fake.last("evaluate")["script"].as_str().unwrap().to_owned();
    assert!(
        script.ends_with(r#"(60, 40, "", "[data-tc-seen=\"2\"]")"#),
        "{script}"
    );

    surface.observe("", Some("seen:1"), Depth::Full).unwrap();
    let scoped = fake.last("evaluate")["script"].as_str().unwrap().to_owned();
    assert!(scoped.contains(r#"("[data-tc-seen=\"1\"]", {"#));
}

#[test]
fn the_surface_keeps_what_its_last_sight_reading_left_out_as_noise() {
    let Harness { surface, .. } = harness("sight-denoised", sighted_fake());
    assert_eq!(surface.denoised(), Denoised::default(), "nothing read yet");
    surface.observe("", None, Depth::Full).unwrap();
    assert_eq!(
        surface.denoised(),
        Denoised {
            ads: 2,
            empty: 1,
            hidden: 0
        }
    );

    let Harness { surface, .. } = harness("tree-denoised", page_fake());
    surface.observe("", None, Depth::Full).unwrap();
    assert_eq!(
        surface.denoised(),
        Denoised::default(),
        "a page read by the tree was not denoised"
    );
}

#[test]
fn the_tree_is_read_when_sight_fails_or_is_turned_off() {
    let Harness { fake, surface, .. } = harness("sight-fallback", page_fake());
    let screen = surface.observe("", None, Depth::Full).unwrap();
    assert_eq!(screen.candidates[0].ref_id, "e1");
    assert!(fake.actions().iter().any(|action| action == "evaluate"));

    let Harness { fake, surface, .. } = harness("sight-off", sighted_fake());
    let surface = surface.with_perception(Perception::Tree);
    surface.observe("", None, Depth::Full).unwrap();
    assert!(!fake.actions().iter().any(|action| action == "evaluate"));
    assert!(format!("{surface:?}").contains("Tree"));
}
