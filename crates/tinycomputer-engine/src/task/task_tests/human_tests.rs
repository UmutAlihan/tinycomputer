//! Tests for walls only a person can pass, and for releasing a task's
//! surfaces when it ends another way.

use super::*;

#[tokio::test]
async fn a_human_wall_pauses_for_a_person_and_the_step_runs_again() {
    let (tasks, script) = controller(vec![
        failed_at_step_two(),
        finished_run(FlowStopReason::Completed, vec![], &[], None),
    ]);
    *script.screen.lock().unwrap() = vec![
        "Security check".to_owned(),
        "Verify you are human".to_owned(),
    ];
    let view = start(
        &tasks,
        json!({"app": "browser", "steps": [
            {"browse": "https://flights.test"},
            "search for flights",
            "open the cheapest result"
        ]}),
        &[],
    );
    let paused = settle(&tasks, &view.id).await;
    let TaskStatus::NeedsHuman { reason, .. } = &paused.status else {
        panic!("{:?}", paused.status);
    };
    assert!(reason.starts_with("prove you are human"), "{reason}");
    assert_eq!(paused.next, ["ContinueTask", "CancelTask", "TaskReport"]);
    assert!(paused.summary.contains("A person is needed"));
    assert!(
        script.released.lock().unwrap().is_empty(),
        "the page stays open for the person"
    );

    let resumed = tasks.continue_task(ContinueTaskRequest {
        id: view.id.clone(),
        answer: Some("done".to_owned()),
        ..ContinueTaskRequest::default()
    });
    assert_eq!(resumed.data.unwrap().status, TaskStatus::Running);
    assert!(matches!(
        settle(&tasks, &view.id).await.status,
        TaskStatus::Done { .. }
    ));
    let requests = script.requests.lock().unwrap();
    assert_eq!(requests[1].flow.app, "browser");
    assert_eq!(
        requests[1].flow.steps.len(),
        2,
        "the failed step and the rest"
    );
    assert_eq!(
        *script.released.lock().unwrap(),
        std::slice::from_ref(&view.id)
    );
}

#[tokio::test]
async fn an_ordinary_failure_or_cancel_releases_the_tasks_surfaces() {
    let (tasks, script) = controller(vec![failed_at_step_two()]);
    *script.screen.lock().unwrap() = vec!["Flights from Delhi".to_owned()];
    let failed = start(&tasks, json!({"app": "browser", "steps": ["a", "b"]}), &[]);
    assert!(matches!(
        settle(&tasks, &failed.id).await.status,
        TaskStatus::Failed { .. }
    ));
    let retry = tasks.continue_task(ContinueTaskRequest {
        id: failed.id.clone(),
        answer: Some("done".to_owned()),
        ..ContinueTaskRequest::default()
    });
    assert_eq!(code(&retry), "NOT_WAITING");

    let running = start(&tasks, json!({"app": "Mail", "steps": ["a"]}), &[]);
    assert!(tasks.cancel(&running.id).ok);
    assert_eq!(*script.released.lock().unwrap(), [failed.id, running.id]);
}

#[tokio::test]
async fn a_budget_failure_is_never_mistaken_for_a_human_wall() {
    let (tasks, script) = controller(vec![finished_run(
        FlowStopReason::ActionBudget,
        vec![],
        &[],
        None,
    )]);
    *script.screen.lock().unwrap() = vec!["Enter the OTP".to_owned()];
    let view = start(&tasks, json!({"app": "Mail", "steps": ["a"]}), &[]);
    assert!(matches!(
        settle(&tasks, &view.id).await.status,
        TaskStatus::Failed { .. }
    ));
}
