//! Tests for the bounded goal loop: its turns, its terminal outcomes, and
//! its action, model, and stall budgets.

use super::*;

#[tokio::test]
async fn goal_loop_executes_a_safe_choice_then_stops_done() {
    let runtime = runtime(vec![
        response("CLICK", 0.9, "1"),
        response("DONE", 0.9, "none"),
    ]);
    let (backend, operations) = backend(4);
    let reply = run_goal_with(
        backend,
        runtime,
        RunGoalRequest {
            app: "Spotify".to_owned(),
            goal: "play the topmost song".to_owned(),
            max_steps: 3,
            max_model_calls: 3,
            ..RunGoalRequest::default()
        },
    )
    .await;
    assert!(reply.ok);
    let result: tinycomputer_bus::JevRunResult =
        serde_json::from_value(reply.data.expect("run returns data")).expect("result decodes");
    assert_eq!(result.stop, JevStopReason::Done);
    assert_eq!(result.turns.len(), 1);
    assert_eq!(
        *operations.lock().expect("operation lock"),
        vec![JevOperation::Click]
    );
    assert_eq!((result.metrics.calls, result.metrics.attempts), (2, 2));
}

#[tokio::test]
async fn scoped_task_executes_two_consequential_steps_in_one_call_without_confirmations() {
    let runtime = runtime(vec![
        response_with("CLICK", 0.94, "1", 0.94, 0.9),
        response_with("CLICK", 0.94, "1", 0.94, 0.9),
    ]);
    let (backend, operations) = backend(6);
    {
        let mut screens = backend.screens.lock().unwrap();
        for screen in screens.iter_mut().skip(2).take(3) {
            screen.candidates[0].name = Some("Second Step".into());
        }
        screens[5].candidates[0].name = Some("Finished".into());
    }
    let reply = run_goal_with(
        backend,
        runtime,
        RunGoalRequest {
            app: "Spotify".into(),
            goal: "complete the two step task".into(),
            window: Some("Liked Songs".into()),
            allowed_operations: vec![JevOperation::Click],
            allowed_targets: vec!["Play First Song by Artist".into(), "Second Step".into()],
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
    assert_eq!(result.turns.len(), 2);
    assert!(result.confirmation_id.is_none());
    assert_eq!(
        *operations.lock().unwrap(),
        vec![JevOperation::Click, JevOperation::Click]
    );
}

#[tokio::test]
async fn low_operation_probability_cannot_be_rescued_by_a_certain_target() {
    let runtime = runtime(vec![
        response_with("CLICK", 0.60, "1", 0.99, 0.0),
        response_with("CLICK", 0.60, "1", 0.99, 0.0),
    ]);
    let (backend, operations) = backend(2);
    let reply = run_goal_with(
        backend,
        runtime,
        RunGoalRequest {
            app: "Spotify".into(),
            goal: "click the play button".into(),
            ..RunGoalRequest::default()
        },
    )
    .await;
    let result: tinycomputer_bus::JevRunResult =
        serde_json::from_value(reply.data.unwrap()).unwrap();
    assert_eq!(result.stop, JevStopReason::LowConfidence);
    assert!(operations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn jev_done_cannot_claim_completion_without_visible_evidence() {
    let (runtime, requests) = runtime_recording(vec![
        response("DONE", 0.9, "none"),
        response("DONE", 0.9, "none"),
    ]);
    let (backend, operations) = backend(2);
    let reply = run_goal_with(
        backend,
        runtime,
        RunGoalRequest {
            app: "Spotify".into(),
            goal: "finish the task".into(),
            success: vec![VisiblePredicate::NamePresent {
                name: "Finished".into(),
            }],
            ..RunGoalRequest::default()
        },
    )
    .await;
    let result: tinycomputer_bus::JevRunResult =
        serde_json::from_value(reply.data.unwrap()).unwrap();
    assert_eq!(result.stop, JevStopReason::VerificationFailed);
    assert!(!result.verified);
    assert!(operations.lock().unwrap().is_empty());
    assert!(
        requests.lock().unwrap()[1].state["recent_actions"][0]
            .as_str()
            .unwrap()
            .contains("required visible success conditions are not yet met")
    );
}

async fn run_case(
    bodies: Vec<tinyinference_decisions::EvaluationResult>,
    screen_count: usize,
    max_steps: u32,
    max_model_calls: u32,
    goal: &str,
) -> tinycomputer_bus::JevRunResult {
    let runtime = runtime(bodies);
    let (backend, _) = backend(screen_count * 3);
    let reply = run_goal_with(
        backend,
        runtime,
        RunGoalRequest {
            app: "Spotify".to_owned(),
            goal: goal.to_owned(),
            max_steps,
            max_model_calls,
            ..RunGoalRequest::default()
        },
    )
    .await;
    serde_json::from_value(reply.data.expect("run returns data")).expect("result decodes")
}

#[tokio::test]
async fn goal_loop_reports_terminal_policy_outcomes() {
    let blocked = run_case(
        vec![response("BLOCKED", 0.9, "none")],
        1,
        3,
        3,
        "impossible",
    )
    .await;
    assert_eq!(blocked.stop, JevStopReason::Blocked);

    let confirmation = run_case(
        vec![response_with("CLICK", 0.9, "1", 0.9, 0.9)],
        1,
        3,
        3,
        "play the topmost song",
    )
    .await;
    assert_eq!(confirmation.stop, JevStopReason::ConfirmationRequired);

    let no_target = run_case(
        vec![response("CLICK", 0.9, "none")],
        1,
        3,
        3,
        "activate something",
    )
    .await;
    assert_eq!(no_target.stop, JevStopReason::LowConfidence);
}

#[tokio::test]
async fn goal_loop_enforces_action_model_and_stall_budgets() {
    let action_budget = run_case(
        vec![response("CLICK", 0.9, "1")],
        2,
        1,
        3,
        "play the topmost song",
    )
    .await;
    assert_eq!(action_budget.stop, JevStopReason::ActionBudget);

    let model_budget = run_case(
        vec![response("CLICK", 0.9, "1")],
        2,
        4,
        1,
        "play the topmost song",
    )
    .await;
    assert_eq!(model_budget.stop, JevStopReason::ModelBudget);

    let stalled = run_case(
        vec![
            response("CLICK", 0.9, "1"),
            response("CLICK", 0.9, "1"),
            response("CLICK", 0.9, "1"),
        ],
        6,
        4,
        4,
        "play the topmost song",
    )
    .await;
    assert_eq!(stalled.stop, JevStopReason::Stalled);
}

#[tokio::test]
async fn goal_loop_preserves_failed_actions_and_post_action_observation_failures() {
    let failed_runtime = runtime(vec![response("CLICK", 0.9, "1")]);
    let operations = Arc::new(Mutex::new(Vec::new()));
    let failed_backend = FakeBackend {
        screens: Arc::new(Mutex::new(VecDeque::from([
            clickable_screen(),
            clickable_screen(),
        ]))),
        operations: Arc::clone(&operations),
        fail_execute: true,
    };
    let failed = run_goal_with(
        failed_backend,
        failed_runtime,
        RunGoalRequest {
            app: "Spotify".to_owned(),
            goal: "play the topmost song".to_owned(),
            ..RunGoalRequest::default()
        },
    )
    .await;
    let failed: tinycomputer_bus::JevRunResult =
        serde_json::from_value(failed.data.expect("failed run data")).expect("result decodes");
    assert_eq!(failed.stop, JevStopReason::ActionUncertain);
    assert_eq!(failed.turns.len(), 1);
    assert!(!failed.turns[0].ok);

    let observation_runtime = runtime(vec![response("CLICK", 0.9, "1")]);
    let (backend, _) = backend(2);
    let lost_screen = run_goal_with(
        backend,
        observation_runtime,
        RunGoalRequest {
            app: "Spotify".to_owned(),
            goal: "play the topmost song".to_owned(),
            ..RunGoalRequest::default()
        },
    )
    .await;
    let lost_screen: tinycomputer_bus::JevRunResult =
        serde_json::from_value(lost_screen.data.expect("lost screen data"))
            .expect("result decodes");
    assert_eq!(lost_screen.stop, JevStopReason::ActionUncertain);
    assert_eq!(lost_screen.turns.len(), 1);
}

#[tokio::test]
async fn unlabeled_textedit_field_completes_scoped_text_task_in_one_call() {
    let text_choice = evaluation(json!({
        "model":"typesafe/jev-1.13-20260917",
        "answers":{
            "operation":{"type":"choice","choice":"TYPE_TEXT","confidence":0.9,
                "probabilities":{"TYPE_TEXT":0.95,"DONE":0.03,"BLOCKED":0.02}},
            "type_text_target":{"type":"choice","choice":"1","confidence":0.9,
                "probabilities":{"1":0.95,"none":0.05}},
            "destructive":{"type":"noul","noul":0.05}
        }, "usage":{"input_tokens":10,"output_tokens":2}
    }));
    let runtime = runtime(vec![text_choice]);
    let (inner, operations) = backend(3);
    {
        let mut screens = inner.screens.lock().unwrap();
        for screen in screens.iter_mut() {
            screen.app = "TextEdit".into();
            screen.window = Some("Untitled".into());
            screen.candidates[0].role = "text field".into();
            screen.candidates[0].name = None;
            screen.candidates[0].native_id = Some(NativeId {
                kind: "ax_identifier".into(),
                value: "First Text View".into(),
            });
            screen.candidates[0].available_actions = vec!["SetValue".into()];
            screen.candidates[0].value = Some(json!(""));
        }
        screens[2].candidates[0].value = Some(json!("marker"));
    }
    let values = Arc::new(Mutex::new(Vec::new()));
    let backend = RecordingTextBackend {
        inner,
        values: Arc::clone(&values),
    };
    let result = run_goal_with(
        backend,
        runtime,
        RunGoalRequest {
            app: "TextEdit".into(),
            goal: "place marker in First Text View".into(),
            window: Some("Untitled".into()),
            allowed_operations: vec![JevOperation::TypeText],
            allowed_targets: vec!["First Text View".into()],
            text_slots: BTreeMap::from([("First Text View".into(), "marker".into())]),
            success: vec![VisiblePredicate::ValueContains {
                name: "First Text View".into(),
                value: "marker".into(),
            }],
            require_confirmations: false,
            ..RunGoalRequest::default()
        },
    )
    .await;
    let result: tinycomputer_bus::JevRunResult =
        serde_json::from_value(result.data.unwrap()).unwrap();
    assert_eq!(result.stop, JevStopReason::Done);
    assert!(result.verified);
    assert_eq!(result.turns.len(), 1);
    assert_eq!(
        result.turns[0].target.as_ref().unwrap().name.as_deref(),
        Some("First Text View")
    );
    assert_eq!(*operations.lock().unwrap(), vec![JevOperation::TypeText]);
    assert_eq!(*values.lock().unwrap(), vec![Some("marker".into())]);
}
