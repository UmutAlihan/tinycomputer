//! Tests for confirming, declining, and expiring a held goal action, and
//! for continuing the goal once it is approved.

use super::*;

#[tokio::test]
async fn approved_goal_action_reobserves_then_continues_once() {
    let (runtime, requests) = runtime_recording(vec![
        response_with("CLICK", 0.9, "1", 0.9, 0.9),
        response("DONE", 0.9, "none"),
    ]);
    let (backend, operations) = backend(4);
    {
        let mut screens = backend.screens.lock().unwrap();
        for index in [2, 3] {
            screens[index].candidates[0].name = Some("Sent First Song by Artist".to_owned());
        }
    }
    let request = RunGoalRequest {
        app: "Spotify".to_owned(),
        goal: "send the selected item".to_owned(),
        max_steps: 3,
        max_model_calls: 3,
        ..RunGoalRequest::default()
    };
    let stopped = run_goal_with(backend.clone(), runtime.clone(), request).await;
    let stopped: tinycomputer_bus::JevRunResult =
        serde_json::from_value(stopped.data.unwrap()).unwrap();
    assert_eq!(stopped.stop, JevStopReason::ConfirmationRequired);
    assert!(operations.lock().unwrap().is_empty());
    let id = stopped.confirmation_id.expect("confirmation handle");
    let resumed = run_goal_with(
        backend.clone(),
        runtime.clone(),
        RunGoalRequest {
            continuation: Some(GoalContinuation {
                id: id.clone(),
                approve: true,
            }),
            ..RunGoalRequest::default()
        },
    )
    .await;
    let resumed: tinycomputer_bus::JevRunResult =
        serde_json::from_value(resumed.data.unwrap()).unwrap();
    assert_eq!(resumed.stop, JevStopReason::Done);
    assert_eq!(resumed.turns.len(), 1);
    assert!(resumed.turns[0].changed);
    assert_eq!(resumed.metrics.calls, 2);
    assert!(
        requests.lock().unwrap()[1].state["recent_actions"][0]
            .as_str()
            .unwrap()
            .contains("changed=true")
    );
    assert_eq!(*operations.lock().unwrap(), vec![JevOperation::Click]);
    let replay = run_goal_with(
        backend,
        runtime,
        RunGoalRequest {
            continuation: Some(GoalContinuation { id, approve: true }),
            ..RunGoalRequest::default()
        },
    )
    .await;
    assert_eq!(replay.error.unwrap().code, "CONFIRMATION_EXPIRED");
}

#[tokio::test]
async fn continuation_uses_prepared_named_text_instead_of_an_empty_value() {
    let first = evaluation(json!({
        "model":"typesafe/jev-1.13-20260917",
        "answers":{
            "operation":{"type":"choice","choice":"TYPE_TEXT","confidence":0.9,
                "probabilities":{"TYPE_TEXT":0.95,"DONE":0.03,"BLOCKED":0.02}},
            "type_text_target":{"type":"choice","choice":"1","confidence":0.9,
                "probabilities":{"1":0.95,"none":0.05}},
            "destructive":{"type":"noul","noul":0.9}
        }, "usage":{"input_tokens":10,"output_tokens":2}
    }));
    let runtime = runtime(vec![first, response("DONE", 0.9, "none")]);
    let (inner, _) = backend(4);
    for screen in inner.screens.lock().unwrap().iter_mut() {
        screen.candidates[0].role = "text field".into();
        screen.candidates[0].name = Some("Document".into());
        screen.candidates[0].available_actions = vec!["SetValue".into()];
    }
    let values = Arc::new(Mutex::new(Vec::new()));
    let backend = RecordingTextBackend {
        inner,
        values: Arc::clone(&values),
    };
    let stopped = run_goal_with(
        backend.clone(),
        runtime.clone(),
        RunGoalRequest {
            app: "Spotify".into(),
            goal: "fill the document".into(),
            text_slots: BTreeMap::from([("Document".into(), "marker".into())]),
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
    let resumed: tinycomputer_bus::JevRunResult =
        serde_json::from_value(resumed.data.unwrap()).unwrap();
    assert_eq!(resumed.stop, JevStopReason::Done);
    assert_eq!(*values.lock().unwrap(), vec![Some("marker".into())]);
}

#[tokio::test]
async fn continuation_never_replays_after_post_action_observation_is_lost() {
    let runtime = runtime(vec![response_with("CLICK", 0.9, "1", 0.9, 0.9)]);
    let (backend, operations) = backend(2);
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
    let result = run_goal_with(
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
        serde_json::from_value(result.data.unwrap()).unwrap();
    assert_eq!(result.stop, JevStopReason::ActionUncertain);
    assert_eq!(result.turns.len(), 1);
    assert!(result.turns[0].ok);
    assert_eq!(*operations.lock().unwrap(), vec![JevOperation::Click]);
}

#[tokio::test]
async fn confirmation_wait_consumes_the_original_elapsed_budget() {
    let runtime = runtime(vec![response_with("CLICK", 0.9, "1", 0.9, 0.9)]);
    let (backend, operations) = backend(1);
    let stopped = run_goal_with(
        backend.clone(),
        runtime.clone(),
        RunGoalRequest {
            app: "Spotify".into(),
            goal: "send the selected item".into(),
            max_elapsed_ms: 50,
            ..RunGoalRequest::default()
        },
    )
    .await;
    let stopped: tinycomputer_bus::JevRunResult =
        serde_json::from_value(stopped.data.unwrap()).unwrap();
    let id = stopped.confirmation_id.unwrap();
    runtime
        .pending
        .lock()
        .unwrap()
        .get_mut(&id)
        .unwrap()
        .started = Instant::now()
        .checked_sub(Duration::from_millis(100))
        .unwrap();
    let result = run_goal_with(
        backend,
        runtime,
        RunGoalRequest {
            continuation: Some(GoalContinuation { id, approve: true }),
            ..RunGoalRequest::default()
        },
    )
    .await;
    let result: tinycomputer_bus::JevRunResult =
        serde_json::from_value(result.data.unwrap()).unwrap();
    assert_eq!(result.stop, JevStopReason::TimeBudget);
    assert!(operations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn declined_and_stale_goal_actions_never_execute() {
    let runtime = runtime(vec![
        response_with("CLICK", 0.9, "1", 0.9, 0.9),
        response_with("CLICK", 0.9, "1", 0.9, 0.9),
    ]);
    let (backend, operations) = backend(4);
    let request = RunGoalRequest {
        app: "Spotify".to_owned(),
        goal: "send the selected item".to_owned(),
        ..RunGoalRequest::default()
    };
    let declined: tinycomputer_bus::JevRunResult = serde_json::from_value(
        run_goal_with(backend.clone(), runtime.clone(), request.clone())
            .await
            .data
            .unwrap(),
    )
    .unwrap();
    let decline: tinycomputer_bus::JevRunResult = serde_json::from_value(
        run_goal_with(
            backend.clone(),
            runtime.clone(),
            RunGoalRequest {
                continuation: Some(GoalContinuation {
                    id: declined.confirmation_id.unwrap(),
                    approve: false,
                }),
                ..RunGoalRequest::default()
            },
        )
        .await
        .data
        .unwrap(),
    )
    .unwrap();
    assert_eq!(decline.stop, JevStopReason::Cancelled);

    let stopped: tinycomputer_bus::JevRunResult = serde_json::from_value(
        run_goal_with(backend.clone(), runtime.clone(), request)
            .await
            .data
            .unwrap(),
    )
    .unwrap();
    let mut changed = clickable_screen();
    changed.candidates[0].name = Some("Different destructive button".to_owned());
    backend.screens.lock().unwrap().push_front(changed);
    let stale: tinycomputer_bus::JevRunResult = serde_json::from_value(
        run_goal_with(
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
        .await
        .data
        .unwrap(),
    )
    .unwrap();
    assert_eq!(stale.stop, JevStopReason::StaleTarget);
    assert!(operations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn expired_confirmation_handle_never_executes() {
    let runtime = runtime(vec![response_with("CLICK", 0.9, "1", 0.9, 0.9)]);
    let (backend, operations) = backend(1);
    let stopped: tinycomputer_bus::JevRunResult = serde_json::from_value(
        run_goal_with(
            backend.clone(),
            runtime.clone(),
            RunGoalRequest {
                app: "Spotify".to_owned(),
                goal: "send the selected item".to_owned(),
                ..RunGoalRequest::default()
            },
        )
        .await
        .data
        .unwrap(),
    )
    .unwrap();
    let id = stopped.confirmation_id.unwrap();
    runtime
        .pending
        .lock()
        .unwrap()
        .get_mut(&id)
        .unwrap()
        .created = Instant::now()
        .checked_sub(Duration::from_secs(601))
        .unwrap();
    let reply = run_goal_with(
        backend,
        runtime,
        RunGoalRequest {
            continuation: Some(GoalContinuation { id, approve: true }),
            ..RunGoalRequest::default()
        },
    )
    .await;
    assert_eq!(reply.error.unwrap().code, "CONFIRMATION_EXPIRED");
    assert!(operations.lock().unwrap().is_empty());
}
