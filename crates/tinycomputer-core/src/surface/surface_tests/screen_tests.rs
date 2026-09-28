//! Tests for fingerprints, change notes, exact names, target payloads, pointer
//! operations, and the descriptions that tell elements apart.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use serde_json::json;
use tinycomputer_bus::{DesktopResponse, JevOperation};

use crate::surface::{
    Candidate, Depth, Screen, Surface, change_note, deliver_text, describe, element_line,
    exact_named_match, fingerprint, holds, result_families, result_groups, target_payload,
    tokenized, uses_pointer,
};
use super::{clickable_screen, two_candidate_screen};

fn a_fingerprint_ignores_ref_churn_between_snapshots() {
    let first = clickable_screen();
    let mut second = clickable_screen();
    second.candidates[0].ref_id = "@s2:e9".to_owned();
    assert_eq!(fingerprint(&first), fingerprint(&second));
    second.candidates[0].states = vec!["selected".to_owned()];
    assert_ne!(fingerprint(&first), fingerprint(&second));
}

#[test]
fn a_change_note_names_what_appeared_and_what_went_away() {
    let before = clickable_screen();
    let mut after = two_candidate_screen();
    after.candidates.remove(0);
    after.window = Some("Other".to_owned());
    after.surface = "sheet".to_owned();
    after.context = (0..8).map(|index| format!("line {index}")).collect();
    let note = change_note(&before, &after, true);
    assert!(note.contains("window is now \"Other\""), "{note}");
    assert!(note.contains("surface is now sheet"), "{note}");
    assert!(note.contains("and 3 more"), "{note}");
    assert!(note.contains("gone: button \"Play First Song"), "{note}");
    assert_eq!(
        change_note(&before, &before, false),
        "nothing on screen changed"
    );
    let mut same_labels = clickable_screen();
    same_labels.candidates[0].states = vec!["selected".to_owned()];
    assert_eq!(
        change_note(&before, &same_labels, true),
        "the screen changed"
    );
}

#[test]
fn exact_names_and_target_payloads_are_recognised() {
    let liked = Candidate {
        name: Some("Liked Songs".to_owned()),
        ..Candidate::default()
    };
    assert!(exact_named_match("open Liked Songs now", Some(&liked)));
    assert!(!exact_named_match("open Disliked Songs", Some(&liked)));
    assert!(!exact_named_match("open it", None));
    let single = Candidate {
        name: Some("Play".to_owned()),
        ..Candidate::default()
    };
    assert!(
        !exact_named_match("press Play", Some(&single)),
        "one word is not identity"
    );
    let described = target_payload(&Candidate {
        ref_id: "@s:e1".to_owned(),
        role: "button".to_owned(),
        description: Some("described".to_owned()),
        ..Candidate::default()
    });
    assert_eq!(described.name.as_deref(), Some("described"));
}

#[test]
fn only_pointer_operations_use_the_pointer() {
    let pointer = [
        JevOperation::Click,
        JevOperation::Expand,
        JevOperation::Collapse,
        JevOperation::Check,
        JevOperation::Uncheck,
    ];
    for operation in pointer {
        assert!(uses_pointer(operation), "{operation:?}");
    }
    for operation in [
        JevOperation::TypeText,
        JevOperation::Scroll,
        JevOperation::Drill,
        JevOperation::Widen,
        JevOperation::Wait,
        JevOperation::Done,
        JevOperation::Blocked,
    ] {
        assert!(!uses_pointer(operation), "{operation:?}");
    }
}

#[test]
fn a_named_element_keeps_the_description_that_tells_it_apart() {
    // A calendar draws each day as its number; only the page's label says
    // which month it belongs to.
    let day = |description: &str| Candidate {
        role: "button".to_owned(),
        name: Some("18".to_owned()),
        description: Some(description.to_owned()),
        available_actions: vec!["Click".to_owned()],
        ..Candidate::default()
    };
    let september = day("Friday, 18 September 2026");
    let october = day("Sunday, 18 October 2026");
    assert_eq!(
        element_line(&october, false),
        "button \"18\" (Sunday, 18 October 2026)"
    );
    assert_ne!(
        element_line(&september, false),
        element_line(&october, false)
    );
    assert_eq!(
        describe(&october, false)["untrusted_accessibility_data"]["says"],
        "Sunday, 18 October 2026"
    );

    let echoed = Candidate {
        name: Some("Close dialog".to_owned()),
        description: Some("Close".to_owned()),
        ..october.clone()
    };
    assert_eq!(
        element_line(&echoed, false),
        "button \"Close dialog\"",
        "a description the name already says adds nothing"
    );
    assert!(
        describe(&echoed, false)["untrusted_accessibility_data"]
            .get("says")
            .is_none()
    );
    let unnamed = Candidate {
        name: None,
        ..october
    };
    assert_eq!(
        element_line(&unnamed, false),
        "button \"Sunday, 18 October 2026\"",
        "an unnamed element is labelled by its description once"
    );
}

#[test]
fn an_unnamed_element_is_told_apart_by_the_named_container_it_sits_in() {
    let search = Candidate {
        role: "combobox".to_owned(),
        available_actions: vec!["SetValue".to_owned()],
        path: vec![
            "main \"Booking Widget\"".to_owned(),
            "button \"destinationCity Empty POPULAR DESTINATIONS Mumbai Bengaluru Hyderabad Kolkata\"".to_owned(),
            "generic".to_owned(),
        ],
        ..Candidate::default()
    };
    let line = element_line(&search, false);
    assert!(
        line.starts_with("combobox in button \"destinationCity Empty POPULAR DESTINATIONS"),
        "{line}"
    );
    assert!(
        line.ends_with('…'),
        "a long container label is clipped: {line}"
    );
    let described = describe(&search, false);
    assert!(
        described["untrusted_accessibility_data"]["near"]
            .as_str()
            .unwrap()
            .starts_with("button \"destinationCity")
    );

    let named = Candidate {
        name: Some("To".to_owned()),
        ..search.clone()
    };
    assert_eq!(element_line(&named, false), "combobox \"To\"");
    assert!(
        describe(&named, false)["untrusted_accessibility_data"]
            .get("near")
            .is_none()
    );
    let loose = Candidate {
        path: vec!["generic".to_owned()],
        ..search
    };
    assert_eq!(
        element_line(&loose, false),
        "combobox",
        "nothing named to sit in"
    );
}
