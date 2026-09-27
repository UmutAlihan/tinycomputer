//! Tests for the flow's own policy: which controls it must not press.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::{Candidate, Screen, destructive_label, is_destructive, named_in_stop_before};

fn clickable_screen() -> Screen {
    Screen {
        app: "Spotify".to_owned(),
        window: Some("Liked Songs".to_owned()),
        surface: "window".to_owned(),
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
        candidates: vec![Candidate {
            ref_id: "@s1:e1".to_owned(),
            role: "button".to_owned(),
            name: Some("Play First Song by Artist".to_owned()),
            available_actions: vec!["Click".to_owned()],
            bounds: Some(json!({"x": 10.0, "y": 100.0})),
            ..Candidate::default()
        }],
    }
}

#[test]
fn the_denylist_names_irreversible_labels() {
    assert!(destructive_label("send"));
    assert!(destructive_label("delete draft"));
    assert!(!destructive_label("new message"));
}

#[test]
fn a_stop_before_phrase_names_a_control_the_denylist_does_not_cover() {
    let phrases = vec!["discard the draft".to_owned()];
    assert!(named_in_stop_before("Discard", &phrases));
    assert!(named_in_stop_before("discard", &phrases));
    assert!(!named_in_stop_before("Reply", &phrases));
    // Too short to mean anything on its own; must never match by accident.
    assert!(!named_in_stop_before("Go", &phrases));
    assert!(!named_in_stop_before("Reply", &[]));
}

#[test]
fn is_destructive_covers_the_denylist_stop_before_phrases_and_unnamed_sheet_buttons() {
    let mut screen = clickable_screen();
    let discard = Candidate {
        name: Some("Discard".to_owned()),
        ..Candidate::default()
    };
    // Neither on the denylist nor named by any stop_before phrase.
    assert!(!is_destructive(&discard, &screen, &[]));
    // The flow's own words name it, even though the denylist does not.
    assert!(is_destructive(
        &discard,
        &screen,
        &["discard the draft".to_owned()]
    ));
    // The denylist alone is still enough, with no stop_before phrases at all.
    let send = Candidate {
        name: Some("Send".to_owned()),
        ..Candidate::default()
    };
    assert!(is_destructive(&send, &screen, &[]));
    // An unnamed button is only gated inside a confirmation sheet.
    let unnamed = Candidate::default();
    assert!(!is_destructive(&unnamed, &screen, &[]));
    screen.surface = "sheet".to_owned();
    assert!(is_destructive(&unnamed, &screen, &[]));
    assert!(
        !is_destructive(&discard, &screen, &[]),
        "a named, non-denylisted control in a sheet is still safe"
    );
}

#[test]
fn is_destructive_gates_a_generic_control_on_a_payment_screen() {
    let mut screen = clickable_screen();
    let continue_button = Candidate {
        name: Some("Continue".to_owned()),
        ..Candidate::default()
    };
    // No payment evidence yet: an unremarkable control is not gated.
    assert!(!is_destructive(&continue_button, &screen, &[]));
    // A card field on the same screen makes it a payment step, so even a
    // control worded only "Continue" must not be pressed by an ordinary step.
    screen.candidates.push(Candidate {
        role: "textbox".to_owned(),
        name: Some("Card number".to_owned()),
        available_actions: vec!["SetValue".to_owned()],
        ..Candidate::default()
    });
    assert!(is_destructive(&continue_button, &screen, &[]));
    // Filling that form commits to nothing, so its fields and choices are
    // not gated: only the button that submits it is.
    for (role, name) in [
        ("textbox", "Card number"),
        ("combobox", "Expiry month"),
        ("option", "12"),
        ("radio", "Saved card ending 1111"),
    ] {
        let control = Candidate {
            role: role.to_owned(),
            name: Some(name.to_owned()),
            ..Candidate::default()
        };
        assert!(!is_destructive(&control, &screen, &[]), "{role} {name}");
    }
    let pay = Candidate {
        role: "button".to_owned(),
        name: Some("Pay ₹7,346".to_owned()),
        ..Candidate::default()
    };
    assert!(is_destructive(&pay, &screen, &[]));
}
