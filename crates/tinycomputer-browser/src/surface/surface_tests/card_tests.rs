//! Tests for clicking through a result card's own cover, and pressing a
//! selection again through the DOM.

use serde_json::json;
use tinycomputer_bus::JevOperation;
use tinycomputer_core::surface::{Candidate, Surface};

use super::{Harness, harness, node};
use crate::fake::{Fake, failure, ok};

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
    assert!(
        script.ends_with(r#"(60, 40, "Select flight", null)"#),
        "{script}"
    );
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

/// A page that takes every click, and says through `evaluate` whether the
/// DOM click fallback had to press.
fn selecting_fake() -> Fake {
    Fake::scripted(|command| match command["action"].as_str().unwrap() {
        "click" => Some(ok(&json!({}))),
        "evaluate" => Some(ok(&json!({"result": true}))),
        _ => None,
    })
}

#[test]
fn a_tab_click_the_page_ignored_is_pressed_again_through_the_dom() {
    // Emirates' trip tabs ignore a trusted click on a freshly loaded page;
    // the element's own `click()` selects them.
    let Harness { fake, surface, .. } = harness("tab-ignored", selecting_fake());
    let tab = Candidate {
        role: "tab".to_owned(),
        name: Some("One way".to_owned()),
        ..node("seen:15", &["Click"])
    };
    assert!(surface.execute(JevOperation::Click, Some(tab), None).ok);
    let script = fake.last("evaluate")["script"].as_str().unwrap().to_owned();
    assert!(script.contains(r#"[data-tc-seen=\"15\"]"#), "{script}");
    assert!(script.contains(".click()"), "{script}");

    // A button, a tree ref, and a tab already selected are left alone.
    for (reference, role, states) in [
        ("seen:16", "button", vec![]),
        ("e5", "tab", vec![]),
        ("seen:17", "tab", vec!["selected".to_owned()]),
    ] {
        let Harness { fake, surface, .. } = harness("tab-left-alone", selecting_fake());
        let node = Candidate {
            role: role.to_owned(),
            states,
            ..node(reference, &["Click"])
        };
        assert!(surface.execute(JevOperation::Click, Some(node), None).ok);
        assert!(
            !fake.actions().iter().any(|action| action == "evaluate"),
            "{reference} {role}"
        );
    }
}
