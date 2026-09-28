//! Tests for finding what may need clearing before a step.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use super::{MAX_DISTRACTIONS, distractions};
use crate::agentic::flow::view::{Candidate, Screen, signature};

fn button(name: &str, path: &[&str]) -> Candidate {
    Candidate {
        ref_id: format!("@{name}-{}", path.join("/")),
        role: "button".to_owned(),
        name: Some(name.to_owned()),
        available_actions: vec!["Click".to_owned()],
        path: path.iter().map(|label| (*label).to_owned()).collect(),
        ..Candidate::default()
    }
}

fn screen(candidates: Vec<Candidate>) -> Screen {
    Screen {
        app: "Site".to_owned(),
        window: Some("Flights".to_owned()),
        surface: "window".to_owned(),
        candidates,
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
    }
}

fn consent() -> Vec<Candidate> {
    let region = ["main", "region \"Cookie consent\""];
    vec![
        button("Accept all", &region),
        button("Reject all", &region),
        button("Manage settings", &region),
    ]
}

fn content() -> Vec<Candidate> {
    vec![
        button("Search flights", &["main", "form \"Book\""]),
        button("One way", &["main", "form \"Book\""]),
    ]
}

#[test]
fn a_consent_card_is_cleared_with_its_least_committal_control() {
    let mut candidates = content();
    candidates.extend(consent());
    let found = distractions(&screen(candidates), "search for flights", &[], &BTreeSet::new());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].closer.name.as_deref(), Some("Reject all"));
    assert!(found[0].shows.iter().any(|shown| shown.contains("Accept all")));
}

#[test]
fn a_toast_with_a_plain_close_is_a_distraction_without_any_telling_words() {
    let mut candidates = content();
    candidates.push(button("Close", &["main", "region \"Unlimited date changes\""]));
    let found = distractions(&screen(candidates), "search for flights", &[], &BTreeSet::new());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].closer.name.as_deref(), Some("Close"));
}

#[test]
fn ordinary_content_is_not_a_distraction() {
    let mut candidates = content();
    // "Accept" alone, in a region nothing marks as a distraction, is the
    // step's own business: a fare to accept, terms to agree to.
    candidates.push(button("Accept", &["main", "form \"Fare\""]));
    candidates.push(button("Done", &["main", "form \"Passengers\""]));
    assert!(distractions(&screen(candidates), "choose the fare", &[], &BTreeSet::new()).is_empty());
}

#[test]
fn a_distraction_the_step_names_is_the_steps_business() {
    let mut candidates = content();
    candidates.extend(consent());
    let found = distractions(
        &screen(candidates),
        "dismiss the cookie banner by accepting essential cookies only",
        &[],
        &BTreeSet::new(),
    );
    assert!(found.is_empty());
}

#[test]
fn a_control_already_pressed_or_irreversible_is_never_offered() {
    let mut candidates = content();
    candidates.extend(consent());
    let pressed = BTreeSet::from([signature(&consent()[1])]);
    let found = distractions(&screen(candidates.clone()), "search", &[], &pressed);
    assert_eq!(found[0].closer.name.as_deref(), Some("Accept all"));
    let found = distractions(
        &screen(candidates),
        "search",
        &["accept all the terms".to_owned(), "reject all offers".to_owned()],
        &BTreeSet::new(),
    );
    assert!(found.is_empty(), "controls the flow names in stop_before are irreversible");
}

#[test]
fn at_most_a_handful_of_distractions_are_offered_front_regions_first() {
    let mut candidates = content();
    for index in 0..6 {
        candidates.push(button(
            "Close",
            &["main", &format!("region \"Promo {index}\"")],
        ));
    }
    candidates.push(button("Close", &["main", "dialog \"Sign in\""]));
    let found = distractions(&screen(candidates), "search", &[], &BTreeSet::new());
    assert_eq!(found.len(), MAX_DISTRACTIONS);
    assert!(found[0].name.contains("Sign in"), "{}", found[0].name);
}
