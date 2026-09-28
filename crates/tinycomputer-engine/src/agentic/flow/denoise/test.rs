//! Tests for deterministic denoising.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::{Tier, compact, inert, oscillates, rank, tier};
use crate::agentic::flow::view::Candidate;

fn element(name: &str, states: &[&str]) -> Candidate {
    Candidate {
        ref_id: format!("@{name}"),
        role: "button".to_owned(),
        name: Some(name.to_owned()),
        states: states.iter().map(|state| (*state).to_owned()).collect(),
        available_actions: vec!["Click".to_owned()],
        path: vec!["window".to_owned()],
        ..Candidate::default()
    }
}

#[test]
fn in_view_elements_outrank_offscreen_and_covered_ones() {
    let pool = vec![
        element("Behind", &["covered"]),
        element("Below", &["offscreen"]),
        element("Here", &[]),
        element("Also here", &["focused"]),
    ];
    let ranked = rank(pool)
        .into_iter()
        .map(|candidate| candidate.name.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(ranked, ["Here", "Also here", "Below", "Behind"]);
    assert_eq!(tier(&element("x", &["Offscreen"])), Tier::Offscreen);
}

#[test]
fn disabled_and_empty_elements_are_left_out() {
    let mut empty = element("Pixel", &[]);
    empty.bounds = Some(json!({"x": 10.0, "y": 10.0, "width": 0.0, "height": 1.0}));
    let mut sized = element("Real", &[]);
    sized.bounds = Some(json!({"x": 10.0, "y": 10.0, "width": 40.0, "height": 20.0}));
    assert!(inert(&element("Off", &["disabled"])));
    assert!(inert(&empty));
    assert!(!inert(&sized));
    assert!(!inert(&element("Unmeasured", &[])));
    let ranked = rank(vec![element("Off", &["disabled"]), empty, sized]);
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].name.as_deref(), Some("Real"));
}

#[test]
fn a_control_exposed_twice_is_offered_once() {
    let outer = element("Search", &[]);
    let mut inner = element("Search", &[]);
    inner.ref_id = "@inner".to_owned();
    inner.path = vec!["window".to_owned(), "button \"Search\"".to_owned()];
    let mut elsewhere = element("Search", &[]);
    elsewhere.ref_id = "@elsewhere".to_owned();
    elsewhere.path = vec!["window".to_owned(), "toolbar".to_owned()];
    let ranked = rank(vec![outer, inner, elsewhere]);
    let refs = ranked
        .iter()
        .map(|candidate| candidate.ref_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(refs, ["@Search", "@elsewhere"]);
}

#[test]
fn an_oscillation_is_a_return_to_two_turns_ago() {
    let seen = ["a".to_owned(), "b".to_owned()];
    assert!(oscillates(&seen, "a"));
    assert!(!oscillates(&seen, "b"));
    assert!(!oscillates(&seen, "c"));
    assert!(!oscillates(&["a".to_owned()], "a"));
    assert!(!oscillates(&["a".to_owned(), "a".to_owned()], "a"));
}

#[test]
fn repeated_history_lines_read_once_with_a_count() {
    let history = ["pressed x", "pressed x", "pressed x", "waited", "pressed x"]
        .map(str::to_owned);
    assert_eq!(
        compact(&history),
        ["pressed x (x3)", "waited", "pressed x"].map(str::to_owned)
    );
    assert!(compact(&[]).is_empty());
}
