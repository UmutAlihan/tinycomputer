//! Tests for the deterministic gates, the action space, and the Jev requests
//! and answers the goal and intent loops share.

use super::*;

#[test]
fn execution_gates_on_selected_probability_not_distribution_concentration() {
    let answer = Answer::Choice(ChoiceAnswer {
        choice: "liked".to_owned(),
        probabilities: BTreeMap::from([("liked".to_owned(), 0.91), ("other".to_owned(), 0.09)]),
        confidence: 0.41,
    });

    assert_eq!(choice(Some(&answer)), Some(("liked", 0.91)));
}

#[test]
fn destructive_actions_always_require_confirmation() {
    assert_eq!(
        gate_with_evidence(JevOperation::Click, 0.99, DESTRUCTIVE, false),
        JevDecisionKind::ConfirmationRequired
    );
}

#[test]
fn only_safe_confident_actions_are_executable() {
    assert_eq!(
        gate_with_evidence(JevOperation::Click, ACT, DESTRUCTIVE - 0.01, false),
        JevDecisionKind::Act
    );
    assert_eq!(
        gate_with_evidence(JevOperation::Click, FLOOR - 0.01, 0.0, false),
        JevDecisionKind::Abstain
    );
}

#[test]
fn an_exact_multiword_accessible_name_is_strong_identity_evidence() {
    let candidate = Candidate {
        name: Some("Liked Songs".to_owned()),
        ..Candidate::default()
    };
    assert!(exact_named_match(
        "open Liked Songs and play the first track",
        Some(&candidate)
    ));
    assert_eq!(
        gate_with_evidence(JevOperation::Click, 0.52, 0.05, true),
        JevDecisionKind::Act
    );
}

#[test]
fn topmost_play_target_is_strong_positional_evidence() {
    let top = Candidate {
        ref_id: "@s:e1".to_owned(),
        name: Some("Play First Song by Artist".to_owned()),
        bounds: Some(serde_json::json!({"x": 10.0, "y": 100.0})),
        ..Candidate::default()
    };
    let lower = Candidate {
        ref_id: "@s:e2".to_owned(),
        name: Some("Play Second Song by Artist".to_owned()),
        bounds: Some(serde_json::json!({"x": 10.0, "y": 160.0})),
        ..Candidate::default()
    };
    let peers = BTreeMap::from([("1".to_owned(), top.clone()), ("2".to_owned(), lower)]);

    assert!(positional_match(
        "play the topmost song",
        Some(&top),
        Some(&peers)
    ));
    assert_eq!(
        gate_with_evidence(JevOperation::Click, 0.49, 0.05, true),
        JevDecisionKind::Act
    );
}

#[test]
fn visible_pause_on_the_top_track_completes_a_playing_goal() {
    let mut screen = two_candidate_screen();
    screen.candidates[0].name = Some("Pause First Song by Artist".to_owned());
    assert!(playing_goal_satisfied(
        "ensure the topmost song is playing",
        &screen
    ));
    assert!(visible_completion("ensure the topmost song is playing", &screen).is_some());
    assert!(!playing_goal_satisfied("open the playlist", &screen));
}

#[test]
fn terminal_operations_are_not_treated_as_actions() {
    assert_eq!(
        gate_with_evidence(JevOperation::Done, 1.0, 1.0, false),
        JevDecisionKind::Done
    );
    assert_eq!(
        gate_with_evidence(JevOperation::Blocked, 1.0, 1.0, false),
        JevDecisionKind::Blocked
    );
    assert_eq!(
        gate_with_evidence(JevOperation::Done, ACT - 0.01, 0.0, false),
        JevDecisionKind::Abstain
    );
}

#[test]
fn deterministic_risk_and_identity_checks_fail_closed() {
    let delete = Candidate {
        name: Some("Delete account".to_owned()),
        ..Candidate::default()
    };
    assert!(deterministic_destructive(
        "continue",
        JevOperation::Click,
        Some(&delete)
    ));
    assert!(!deterministic_destructive(
        "continue",
        JevOperation::Scroll,
        Some(&delete)
    ));
    let candidate = Candidate {
        name: Some("Liked Songs".to_owned()),
        ..Candidate::default()
    };
    assert!(!exact_named_match("open Disliked Songs", Some(&candidate)));
    let decorated = Candidate {
        name: Some("Liked Songs Pinned Downloaded Playlist".to_owned()),
        ..Candidate::default()
    };
    assert!(exact_named_match("open Liked Songs", Some(&decorated)));
}

#[test]
fn action_space_and_requests_cover_every_supported_capability() {
    let screen = Screen {
        app: "App".to_owned(),
        window: Some("Window".to_owned()),
        window_id: None,
        surface: "window".to_owned(),
        root: None,
        candidates: vec![Candidate {
            ref_id: "@s:e1".to_owned(),
            role: "control".to_owned(),
            name: Some("Everything".to_owned()),
            value: Some(json!("held")),
            states: vec!["checked".to_owned()],
            available_actions: vec![
                "Click".to_owned(),
                "SetValue".to_owned(),
                "Toggle".to_owned(),
                "Expand".to_owned(),
                "Collapse".to_owned(),
                "Scroll".to_owned(),
            ],
            children_count: Some(3),
            ..Candidate::default()
        }],
        observed: Vec::new(),
    };
    let space = action_space(&screen, true);
    for operation in [
        "CLICK",
        "TYPE_TEXT",
        "CHECK",
        "UNCHECK",
        "EXPAND",
        "COLLAPSE",
        "SCROLL",
        "DRILL",
    ] {
        assert!(space.targets.contains_key(operation), "missing {operation}");
    }
    let evaluation = request("jev-latest", "change it", &screen, &space, &[], true);
    assert!(evaluation.questions.contains_key("type_text_target"));
    let rerank = rerank_request(
        "jev-latest",
        "change it",
        &screen,
        "CLICK",
        space.targets.get("CLICK").expect("click targets"),
        true,
    );
    assert_eq!(rerank.questions.len(), 1);
}

#[test]
fn answer_helpers_cover_terminal_missing_and_shortlist_paths() {
    let screen = clickable_screen();
    let space = action_space(&screen, false);
    let answer = Answer::Choice(ChoiceAnswer {
        choice: "1".to_owned(),
        probabilities: BTreeMap::from([("1".to_owned(), 0.8), ("none".to_owned(), 0.2)]),
        confidence: 0.2,
    });
    assert!(target(&space, "CLICK", Some(&answer)).is_some());
    assert_eq!(shortlist(&space, "CLICK", Some(&answer)).len(), 1);
    assert!(shortlist(&space, "MISSING", Some(&answer)).is_empty());
    assert!(choice(None).is_none());
    assert!((noul(None) - 1.0).abs() < f64::EPSILON);
    for (wire, operation) in [
        ("CLICK", JevOperation::Click),
        ("TYPE_TEXT", JevOperation::TypeText),
        ("CHECK", JevOperation::Check),
        ("UNCHECK", JevOperation::Uncheck),
        ("EXPAND", JevOperation::Expand),
        ("COLLAPSE", JevOperation::Collapse),
        ("SCROLL", JevOperation::Scroll),
        ("DRILL", JevOperation::Drill),
        ("WIDEN", JevOperation::Widen),
        ("WAIT", JevOperation::Wait),
        ("DONE", JevOperation::Done),
        ("BLOCKED", JevOperation::Blocked),
    ] {
        assert_eq!(parse_operation(wire), Some(operation));
    }
    assert_eq!(parse_operation("NOPE"), None);
}
