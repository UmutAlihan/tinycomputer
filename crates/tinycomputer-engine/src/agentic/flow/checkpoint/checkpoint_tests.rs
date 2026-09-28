//! Tests for checkpoints and reversibility; the undo ladder runs in the flow
//! simulator (`flow/test/deliberation.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{Checkpoint, RESTORED, Reversibility, classify};
use crate::agentic::flow::{
    expect::Effect,
    view::{Candidate, Screen},
};

fn element(role: &str, name: &str) -> Candidate {
    Candidate {
        ref_id: format!("@{name}"),
        role: role.to_owned(),
        name: Some(name.to_owned()),
        available_actions: vec!["Click".to_owned()],
        ..Candidate::default()
    }
}

fn screen(names: &[&str]) -> Screen {
    Screen {
        app: "Site".to_owned(),
        window: Some("Page".to_owned()),
        surface: "window".to_owned(),
        candidates: names.iter().map(|name| element("button", name)).collect(),
        context: vec!["Heading".to_owned()],
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
    }
}

#[test]
fn actions_are_classified_by_how_they_undo() {
    let page = screen(&["Next"]);
    assert_eq!(
        classify(&Effect::Opens, &element("button", "More"), &page, &[]),
        Reversibility::Reversible
    );
    assert_eq!(
        classify(
            &Effect::Toggles(true),
            &element("radio", "Economy"),
            &page,
            &[]
        ),
        Reversibility::Restorable
    );
    assert_eq!(
        classify(&Effect::Navigates, &element("link", "Details"), &page, &[]),
        Reversibility::Restorable
    );
    assert_eq!(
        classify(&Effect::Unknown, &element("button", "Send"), &page, &[]),
        Reversibility::Irreversible
    );
    assert_eq!(
        classify(
            &Effect::Unknown,
            &element("button", "Go ahead"),
            &page,
            &["go ahead with the order".to_owned()]
        ),
        Reversibility::Irreversible
    );
}

#[test]
fn a_screen_is_restored_when_most_of_it_is_back_at_the_same_address() {
    let before = screen(&["A", "B", "C", "D"]);
    let checkpoint = Checkpoint::of(&before, Some("https://site.test/a"));
    assert!((checkpoint.similarity(&before) - 1.0).abs() < 1e-9);
    assert!(checkpoint.restored(&before, Some("https://site.test/a")));
    assert!(!checkpoint.restored(&before, Some("https://site.test/b")));
    let mostly = screen(&["A", "B", "C", "D", "E"]);
    assert!(checkpoint.restored(&mostly, Some("https://site.test/a")));
    let changed = screen(&["A", "X", "Y", "Z"]);
    assert!(checkpoint.similarity(&changed) < RESTORED);
    assert!(!checkpoint.restored(&changed, Some("https://site.test/a")));
}

#[test]
fn a_desktop_checkpoint_has_no_address_and_an_empty_one_matches_anything() {
    let before = screen(&["A"]);
    let checkpoint = Checkpoint::of(&before, None);
    assert!(checkpoint.location.is_none());
    assert!(checkpoint.restored(&before, Some("anything")));
    let mut blank = screen(&[]);
    blank.context.clear();
    let empty = Checkpoint::of(&blank, None);
    assert!((empty.similarity(&before) - 1.0).abs() < 1e-9);
}
