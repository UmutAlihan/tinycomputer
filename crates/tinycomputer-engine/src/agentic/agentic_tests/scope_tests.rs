//! Tests for keeping a goal inside its scope: the bound window, the allowed
//! targets, and a target that must still be the same when the action runs.

use super::*;

#[tokio::test]
async fn goal_keeps_the_chosen_window_id_through_every_observation() {
    let runtime = runtime(vec![response("CLICK", 0.9, "1")]);
    let (inner, operations) = backend(3);
    {
        let mut screens = inner.screens.lock().unwrap();
        for screen in screens.iter_mut() {
            screen.window_id = Some("w-515619".into());
            screen.window = Some("desktop-e2e-noapproval.txt".into());
        }
        screens[2].candidates[0].name = Some("Finished".into());
    }
    let requested = Arc::new(Mutex::new(Vec::new()));
    let backend = WindowBoundBackend {
        inner,
        requested: Arc::clone(&requested),
    };
    let result = run_goal_with(
        backend,
        runtime,
        RunGoalRequest {
            app: "Spotify".into(),
            goal: "complete one action".into(),
            window: Some("desktop-e2e-noapproval.txt".into()),
            window_id: Some("w-515619".into()),
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
        serde_json::from_value(result.data.unwrap()).unwrap();
    assert!(result.verified);
    assert_eq!(*operations.lock().unwrap(), vec![JevOperation::Click]);
    assert_eq!(*requested.lock().unwrap(), vec![Some("w-515619".into()); 3]);
}

#[tokio::test]
async fn goal_rejects_a_snapshot_from_a_different_window_before_jev() {
    let (backend, operations) = backend(1);
    let result = run_goal_with(
        backend,
        runtime(Vec::new()),
        RunGoalRequest {
            app: "Spotify".into(),
            goal: "complete one action".into(),
            window_id: Some("w-515619".into()),
            ..RunGoalRequest::default()
        },
    )
    .await;
    let result: tinycomputer_bus::JevRunResult =
        serde_json::from_value(result.data.unwrap()).unwrap();
    assert_eq!(result.stop, JevStopReason::ScopeChanged);
    assert!(operations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn missing_bound_window_does_not_fall_back_to_another_window() {
    let (inner, operations) = backend(1);
    let requested = Arc::new(Mutex::new(Vec::new()));
    let backend = WindowBoundBackend {
        inner,
        requested: Arc::clone(&requested),
    };
    let reply = run_goal_with(
        backend,
        runtime(Vec::new()),
        RunGoalRequest {
            app: "Spotify".into(),
            goal: "complete one action".into(),
            window_id: Some("w-missing".into()),
            ..RunGoalRequest::default()
        },
    )
    .await;
    assert_eq!(reply.error.unwrap().code, "WINDOW_NOT_FOUND");
    let requested = requested.lock().unwrap();
    assert!(requested.len() > 1 && requested.len() <= 20);
    assert!(
        requested
            .iter()
            .all(|window| window.as_deref() == Some("w-missing"))
    );
    assert!(operations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn fresh_target_change_prevents_a_mutation() {
    let runtime = runtime(vec![response("CLICK", 0.9, "1")]);
    let (backend, operations) = backend(2);
    backend.screens.lock().unwrap()[1].candidates[0].name = Some("Different button".into());
    let reply = run_goal_with(
        backend,
        runtime,
        RunGoalRequest {
            app: "Spotify".into(),
            goal: "click play".into(),
            ..RunGoalRequest::default()
        },
    )
    .await;
    let result: tinycomputer_bus::JevRunResult =
        serde_json::from_value(reply.data.unwrap()).unwrap();
    assert_eq!(result.stop, JevStopReason::StaleTarget);
    assert!(operations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn continuous_task_rejects_empty_or_blank_scope_before_jev() {
    let (backend, operations) = backend(1);
    let request = RunGoalRequest {
        app: "Spotify".into(),
        goal: "click play".into(),
        allowed_operations: vec![JevOperation::Click],
        allowed_targets: vec!["Play First Song by Artist".into()],
        success: vec![VisiblePredicate::NamePresent {
            name: "Finished".into(),
        }],
        require_confirmations: false,
        ..RunGoalRequest::default()
    };
    for bad in [
        RunGoalRequest {
            allowed_targets: vec![" ".into()],
            ..request.clone()
        },
        RunGoalRequest {
            success: vec![VisiblePredicate::ValueContains {
                name: "Document".into(),
                value: String::new(),
            }],
            ..request.clone()
        },
        RunGoalRequest {
            success: vec![VisiblePredicate::NameContains {
                fragment: String::new(),
                within: "Messages in chat with Alex Rivera".into(),
            }],
            ..request.clone()
        },
        RunGoalRequest {
            success: vec![VisiblePredicate::NameContains {
                fragment: "Hello".into(),
                within: "  ".into(),
            }],
            ..request.clone()
        },
        RunGoalRequest {
            allowed_operations: Vec::new(),
            ..request.clone()
        },
    ] {
        let reply = run_goal_with(backend.clone(), runtime(Vec::new()), bad).await;
        assert_eq!(reply.error.unwrap().code, "INVALID_TASK_SCOPE");
    }
    assert!(operations.lock().unwrap().is_empty());
}

#[test]
fn changed_native_identifier_fails_fresh_target_validation() {
    let mut before = clickable_screen();
    before.candidates[0].name = None;
    before.candidates[0].native_id = Some(NativeId {
        kind: "ax_identifier".into(),
        value: "First Text View".into(),
    });
    let mut after = before.clone();
    after.candidates[0].native_id.as_mut().unwrap().value = "Second Text View".into();
    assert!(!same_target(
        &before,
        &after,
        &before.candidates[0],
        &after.candidates[0],
        JevOperation::Click
    ));
}
