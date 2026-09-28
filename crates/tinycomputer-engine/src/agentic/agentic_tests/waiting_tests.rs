//! Tests for waiting on a starting application and on delayed visible
//! success, without ever replaying an action whose delivery was unverified.

use super::*;

#[tokio::test]
async fn goal_waits_for_a_starting_app_but_not_for_denied_permission() {
    for (code, expected_attempts, done) in [
        ("APP_NOT_FOUND", 2, true),
        ("WINDOW_NOT_FOUND", 2, true),
        ("PERM_DENIED", 1, false),
    ] {
        let (inner, _) = backend(1);
        let attempts = Arc::new(Mutex::new(0));
        let backend = ReadinessBackend {
            inner,
            attempts: Arc::clone(&attempts),
            first_error: code,
        };
        let reply = run_goal_with(
            backend,
            runtime(Vec::new()),
            RunGoalRequest {
                app: "Spotify".into(),
                goal: "verify the visible song".into(),
                success: vec![VisiblePredicate::NamePresent {
                    name: "Play First Song by Artist".into(),
                }],
                ..RunGoalRequest::default()
            },
        )
        .await;
        assert_eq!(*attempts.lock().unwrap(), expected_attempts, "{code}");
        if done {
            let result: tinycomputer_bus::JevRunResult =
                serde_json::from_value(reply.data.unwrap()).unwrap();
            assert_eq!(result.stop, JevStopReason::Done);
            assert!(result.verified);
        } else {
            assert_eq!(reply.error.unwrap().code, code);
        }
    }
}

#[tokio::test]
async fn goal_waits_for_delayed_success_without_replaying_an_unverified_click() {
    let (inner, operations) = backend(4);
    inner.screens.lock().unwrap()[3].candidates[0].name = Some("Finished".into());
    let reply = run_goal_with(
        UnverifiedClickBackend { inner },
        runtime(vec![response("CLICK", 0.95, "1")]),
        RunGoalRequest {
            app: "Spotify".into(),
            goal: "click play and verify finished".into(),
            allowed_operations: vec![JevOperation::Click],
            allowed_targets: vec!["Play First Song by Artist".into()],
            success: vec![VisiblePredicate::NamePresent {
                name: "Finished".into(),
            }],
            require_confirmations: false,
            ..RunGoalRequest::default()
        },
    )
    .await;
    let result: tinycomputer_bus::JevRunResult =
        serde_json::from_value(reply.data.unwrap()).unwrap();
    assert_eq!(result.stop, JevStopReason::Done);
    assert!(result.verified);
    assert_eq!(result.turns.len(), 1);
    assert_eq!(*operations.lock().unwrap(), vec![JevOperation::Click]);
}

#[tokio::test]
async fn unverified_consequential_action_stops_after_settle_without_replay() {
    let (inner, operations) = backend(3);
    let reply = run_goal_with(
        UnverifiedClickBackend { inner },
        runtime(vec![response_with("CLICK", 0.95, "1", 0.95, 0.9)]),
        RunGoalRequest {
            app: "Spotify".into(),
            goal: "send the selected item".into(),
            allowed_operations: vec![JevOperation::Click],
            allowed_targets: vec!["Play First Song by Artist".into()],
            success: vec![VisiblePredicate::NamePresent {
                name: "Finished".into(),
            }],
            max_elapsed_ms: 4_000,
            require_confirmations: false,
            ..RunGoalRequest::default()
        },
    )
    .await;
    let result: tinycomputer_bus::JevRunResult =
        serde_json::from_value(reply.data.unwrap()).unwrap();
    assert_eq!(result.stop, JevStopReason::ActionUncertain);
    assert_eq!(result.turns.len(), 1);
    assert_eq!(*operations.lock().unwrap(), vec![JevOperation::Click]);
}

#[tokio::test]
async fn approved_unverified_consequential_action_without_predicate_never_replays() {
    let (inner, operations) = backend(3);
    let backend = UnverifiedClickBackend { inner };
    let runtime = runtime(vec![response_with("CLICK", 0.95, "1", 0.95, 0.9)]);
    let stopped = run_goal_with(
        backend.clone(),
        runtime.clone(),
        RunGoalRequest {
            app: "Spotify".into(),
            goal: "send the selected item".into(),
            ..RunGoalRequest::default()
        },
    )
    .await;
    let stopped: tinycomputer_bus::JevRunResult =
        serde_json::from_value(stopped.data.unwrap()).unwrap();
    assert_eq!(stopped.stop, JevStopReason::ConfirmationRequired);
    let resumed = run_goal_with(
        backend,
        runtime,
        RunGoalRequest {
            continuation: Some(GoalContinuation {
                id: stopped.confirmation_id.unwrap(),
                approve: true,
            }),
            ..RunGoalRequest::default()
        },
    )
    .await;
    let result: tinycomputer_bus::JevRunResult =
        serde_json::from_value(resumed.data.unwrap()).unwrap();
    assert_eq!(result.stop, JevStopReason::ActionUncertain);
    assert_eq!(result.turns.len(), 1);
    assert_eq!(*operations.lock().unwrap(), vec![JevOperation::Click]);
}

#[tokio::test]
async fn approved_unverified_action_waits_for_delayed_visible_success() {
    let (inner, operations) = backend(4);
    inner.screens.lock().unwrap()[3].candidates[0].name = Some("Finished".into());
    let backend = UnverifiedClickBackend { inner };
    let runtime = runtime(vec![response_with("CLICK", 0.95, "1", 0.95, 0.9)]);
    let stopped = run_goal_with(
        backend.clone(),
        runtime.clone(),
        RunGoalRequest {
            app: "Spotify".into(),
            goal: "send the selected item".into(),
            success: vec![VisiblePredicate::NamePresent {
                name: "Finished".into(),
            }],
            ..RunGoalRequest::default()
        },
    )
    .await;
    let stopped: tinycomputer_bus::JevRunResult =
        serde_json::from_value(stopped.data.unwrap()).unwrap();
    assert_eq!(stopped.stop, JevStopReason::ConfirmationRequired);
    let resumed = run_goal_with(
        backend,
        runtime,
        RunGoalRequest {
            continuation: Some(GoalContinuation {
                id: stopped.confirmation_id.unwrap(),
                approve: true,
            }),
            ..RunGoalRequest::default()
        },
    )
    .await;
    let result: tinycomputer_bus::JevRunResult =
        serde_json::from_value(resumed.data.unwrap()).unwrap();
    assert_eq!(result.stop, JevStopReason::Done);
    assert!(result.verified);
    assert_eq!(result.turns.len(), 1);
    assert_eq!(*operations.lock().unwrap(), vec![JevOperation::Click]);
}
