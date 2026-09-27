//! Deterministic predicate checks against accessibility observations.

use serde_json::json;
use tinydesktop_bus::VisiblePredicate;

use super::{satisfied, verify};
use crate::agentic::screen::{Candidate, Screen};

#[test]
fn checks_named_value_and_state_without_returning_unrequested_values() {
    let screen = Screen {
        app: "TextEdit".into(),
        window: Some("Untitled".into()),
        window_id: None,
        surface: "window".into(),
        root: None,
        candidates: Vec::new(),
        observed: vec![Candidate {
            name: Some("Document".into()),
            value: Some(json!("before marker after")),
            states: vec!["focused".into()],
            ..Candidate::default()
        }],
    };
    let evidence = verify(
        &screen,
        &[
            VisiblePredicate::ValueContains {
                name: "Document".into(),
                value: "marker".into(),
            },
            VisiblePredicate::StateContains {
                name: "Document".into(),
                state: "focused".into(),
            },
        ],
    );
    assert!(satisfied(&evidence));
    assert_eq!(
        evidence.predicates[0].observed_value.as_deref(),
        Some("marker")
    );
    assert!(evidence.predicates[1].observed_value.is_none());
}

#[test]
fn ambiguous_values_do_not_verify() {
    let node = Candidate {
        name: Some("Document".into()),
        value: Some(json!("marker")),
        ..Candidate::default()
    };
    let screen = Screen {
        app: "App".into(),
        window: None,
        window_id: None,
        surface: "window".into(),
        root: None,
        candidates: Vec::new(),
        observed: vec![node.clone(), node],
    };
    let evidence = verify(
        &screen,
        &[VisiblePredicate::ValueEquals {
            name: "Document".into(),
            value: "marker".into(),
        }],
    );
    assert!(!satisfied(&evidence));
}

#[test]
fn description_matches_even_when_a_distinct_name_exists() {
    let screen = Screen {
        app: "App".into(),
        window: None,
        window_id: None,
        surface: "window".into(),
        root: None,
        candidates: Vec::new(),
        observed: vec![Candidate {
            name: Some("Internal name".into()),
            description: Some("User-facing description".into()),
            ..Candidate::default()
        }],
    };
    let evidence = verify(
        &screen,
        &[VisiblePredicate::NamePresent {
            name: "User-facing description".into(),
        }],
    );
    assert!(satisfied(&evidence));
}

#[test]
fn empty_exact_value_is_a_valid_cleared_field_condition() {
    let screen = Screen {
        app: "App".into(),
        window: None,
        window_id: None,
        surface: "window".into(),
        root: None,
        candidates: Vec::new(),
        observed: vec![Candidate {
            name: Some("Search".into()),
            value: Some(json!("")),
            ..Candidate::default()
        }],
    };
    let evidence = verify(
        &screen,
        &[VisiblePredicate::ValueEquals {
            name: "Search".into(),
            value: String::new(),
        }],
    );
    assert!(satisfied(&evidence));
}

#[test]
fn contained_message_fragment_matches_only_the_named_chat() {
    let mut screen = Screen {
        app: "WhatsApp".into(),
        window: None,
        window_id: None,
        surface: "window".into(),
        root: None,
        candidates: Vec::new(),
        observed: vec![Candidate {
            role: "statictext".into(),
            name: Some("Your message, Hello from OpenHuman, Sent to Alex Rivera".into()),
            path: vec!["group \"Messages in chat with Daddy\"".into()],
            ..Candidate::default()
        }],
    };
    let predicate = VisiblePredicate::NameContains {
        fragment: "Your message, Hello from OpenHuman".into(),
        within: "Messages in chat with Alex Rivera".into(),
    };
    assert!(!satisfied(&verify(
        &screen,
        std::slice::from_ref(&predicate)
    )));
    screen.observed[0].path = vec!["group \"Messages in chat with Alex Rivera\"".into()];
    let evidence = verify(&screen, &[predicate]);
    assert!(satisfied(&evidence));
    assert_eq!(
        evidence.predicates[0].observed_name.as_deref(),
        Some("Your message, Hello from OpenHuman, Sent to Alex Rivera")
    );
}
