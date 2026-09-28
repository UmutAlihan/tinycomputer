//! Tests for predicting an action's effect and checking it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{Effect, Outcome, check, find, predict, selected};
use crate::agentic::flow::view::{Candidate, Screen};

fn element(role: &str, name: &str, states: &[&str]) -> Candidate {
    Candidate {
        ref_id: format!("@{name}"),
        role: role.to_owned(),
        name: Some(name.to_owned()),
        states: states.iter().map(|state| (*state).to_owned()).collect(),
        available_actions: vec!["Click".to_owned()],
        path: vec!["form".to_owned()],
        ..Candidate::default()
    }
}

fn screen(surface: &str, window: &str, candidates: Vec<Candidate>) -> Screen {
    Screen {
        app: "Site".to_owned(),
        window: Some(window.to_owned()),
        surface: surface.to_owned(),
        candidates,
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
    }
}

#[test]
fn predictions_follow_the_move_and_the_role() {
    assert_eq!(
        predict("scroll", &element("list", "Rows", &[])),
        Effect::Scrolls
    );
    assert_eq!(
        predict("expand", &element("button", "More", &[])),
        Effect::Opens
    );
    assert_eq!(
        predict("activate", &element("radio", "Economy", &[])),
        Effect::Toggles(true)
    );
    assert_eq!(
        predict("activate", &element("checkbox", "Insurance", &["checked"])),
        Effect::Toggles(false)
    );
    assert_eq!(
        predict("activate", &element("combobox", "From", &[])),
        Effect::Opens
    );
    assert_eq!(
        predict("activate", &element("button", "Filters", &["collapsed"])),
        Effect::Opens
    );
    assert_eq!(
        predict("activate", &element("link", "Details", &[])),
        Effect::Navigates
    );
    assert_eq!(
        predict("activate", &element("button", "Not now", &[])),
        Effect::Closes
    );
    assert_eq!(
        predict("activate", &element("button", "Search", &[])),
        Effect::Unknown
    );
    assert!(Effect::Toggles(true).toggles() && !Effect::Opens.toggles());
    assert!(Effect::Opens.meant("button \"x\"").contains("open"));
}

#[test]
fn a_toggle_that_cleared_instead_of_selecting_is_missed() {
    let before = element("checkbox", "Insurance", &[]);
    let effect = predict("activate", &before);
    let then = screen("window", "Extras", vec![before.clone()]);
    let now = screen("window", "Extras", vec![element("checkbox", "Insurance", &[])]);
    assert!(matches!(
        check(&effect, &before, &then, &now, false),
        Outcome::Missed(_)
    ));
    let ticked = screen(
        "window",
        "Extras",
        vec![element("checkbox", "Insurance", &["checked"])],
    );
    assert_eq!(check(&effect, &before, &then, &ticked, false), Outcome::Met);
    let cleared = Effect::Toggles(false);
    assert!(matches!(
        check(&cleared, &before, &then, &ticked, false),
        Outcome::Missed(_)
    ));
    assert!(matches!(
        check(&effect, &before, &then, &ticked, true),
        Outcome::Missed(_)
    ));
    let gone = screen("window", "Extras", Vec::new());
    assert_eq!(check(&effect, &before, &then, &gone, false), Outcome::Unclear);
}

#[test]
fn opening_that_left_the_page_is_missed_and_opening_that_showed_more_is_met() {
    let target = element("combobox", "From", &[]);
    let then = screen("window", "Search", vec![target.clone()]);
    let open = screen(
        "window",
        "Search",
        vec![target.clone(), element("option", "Delhi", &[])],
    );
    assert_eq!(check(&Effect::Opens, &target, &then, &open, false), Outcome::Met);
    assert!(matches!(
        check(&Effect::Opens, &target, &then, &then, true),
        Outcome::Missed(_)
    ));
    assert_eq!(
        check(&Effect::Opens, &target, &then, &then, false),
        Outcome::Unclear
    );
}

#[test]
fn navigation_and_closing_are_met_by_what_changed() {
    let link = element("link", "Details", &[]);
    let then = screen("window", "Results", vec![link.clone()]);
    let next = screen("window", "Details", Vec::new());
    assert_eq!(check(&Effect::Navigates, &link, &then, &next, false), Outcome::Met);
    assert_eq!(check(&Effect::Navigates, &link, &then, &then, true), Outcome::Met);
    assert_eq!(
        check(&Effect::Navigates, &link, &then, &then, false),
        Outcome::Unclear
    );
    let close = element("button", "Close", &[]);
    let sheet = screen("sheet", "Results", vec![close.clone()]);
    assert_eq!(check(&Effect::Closes, &close, &sheet, &then, false), Outcome::Met);
    assert_eq!(check(&Effect::Unknown, &close, &sheet, &then, true), Outcome::Unclear);
}

#[test]
fn an_element_is_found_again_by_role_name_and_place() {
    let target = element("tab", "One way", &[]);
    let mut moved = target.clone();
    moved.path = vec!["elsewhere".to_owned()];
    let now = screen("window", "x", vec![moved, element("tab", "One way", &["selected"])]);
    assert!(selected(find(&target, &now).unwrap()));
}
