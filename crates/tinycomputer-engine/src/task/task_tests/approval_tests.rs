//! Tests for irreversible actions and payment: approving, declining, and
//! resuming within what the task's budget has left.

use super::*;

fn mail_flow() -> serde_json::Value {
    json!({"app": "Notes", "steps": [
        {"open": "Mail"},
        "start a new email message",
        {"stop_before": "sending the email"},
        "start another email message"
    ]})
}

fn gated(target: &str, phrase: &str) -> DesktopResponse {
    finished_run(
        FlowStopReason::StoppedBeforeDestructive,
        vec![
            step("1", "open", "Mail", StepOutcome::Done, ""),
            step(
                "2",
                "do",
                "start a new email message",
                StepOutcome::Done,
                "",
            ),
            step("3", "stop_before", phrase, StepOutcome::Gated, "found it"),
        ],
        &[],
        Some(target),
    )
}

#[tokio::test]
async fn an_approved_irreversible_action_is_performed_and_the_rest_runs() {
    let (tasks, script) = controller(vec![
        gated("Send", "sending the email"),
        finished_run(FlowStopReason::Completed, vec![], &[], None),
        finished_run(FlowStopReason::Completed, vec![], &[], None),
    ]);
    let view = start(&tasks, mail_flow(), &[]);
    let paused = settle(&tasks, &view.id).await;
    let TaskStatus::NeedsApproval { action, target, .. } = &paused.status else {
        panic!("{:?}", paused.status);
    };
    assert_eq!(
        (action.as_str(), target.as_str()),
        ("sending the email", "Send")
    );
    assert_eq!(paused.next, ["ContinueTask", "CancelTask"]);
    assert!(paused.summary.contains("approve or decline"));

    let unanswered = tasks.continue_task(ContinueTaskRequest {
        id: view.id.clone(),
        ..ContinueTaskRequest::default()
    });
    assert_eq!(code(&unanswered), "APPROVAL_REQUIRED");

    let approved = tasks.continue_task(ContinueTaskRequest {
        id: view.id.clone(),
        approve: Some(true),
        ..ContinueTaskRequest::default()
    });
    assert!(approved.ok);
    assert!(matches!(
        settle(&tasks, &view.id).await.status,
        TaskStatus::Done { .. }
    ));
    let requests = script.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(
        requests[1].allow_destructive,
        "only the approved action may be performed"
    );
    assert_eq!(
        requests[1].flow.app, "Mail",
        "resumes on the app the flow had opened"
    );
    assert_eq!(
        requests[1].flow.steps,
        [FlowStep::Action(FlowAction::StopBefore(
            "sending the email".to_owned()
        ))]
    );
    assert!(!requests[2].allow_destructive);
    assert_eq!(requests[2].flow.steps.len(), 1);
}

#[tokio::test]
async fn an_approval_nested_in_an_if_resumes_the_whole_branch_and_what_follows() {
    // The gated `stop_before` is nested one level inside the `if`, the third
    // top-level step (path "3.1"), so approving it must not silently drop
    // the rest of that branch, or "finish another email" after it.
    let flow_value = json!({"app": "Notes", "steps": [
        {"open": "Mail"},
        "start a new email message",
        {"if": {
            "condition": "a draft is open",
            "then": [{"stop_before": "sending the email"}],
        }},
        "finish another email message"
    ]});
    let (tasks, script) = controller(vec![
        finished_run(
            FlowStopReason::StoppedBeforeDestructive,
            vec![
                step("1", "open", "Mail", StepOutcome::Done, ""),
                step(
                    "2",
                    "do",
                    "start a new email message",
                    StepOutcome::Done,
                    "",
                ),
                step(
                    "3.1",
                    "stop_before",
                    "sending the email",
                    StepOutcome::Gated,
                    "found it",
                ),
            ],
            &[],
            Some("Send"),
        ),
        finished_run(FlowStopReason::Completed, vec![], &[], None),
        finished_run(FlowStopReason::Completed, vec![], &[], None),
    ]);
    let view = start(&tasks, flow_value, &[]);
    settle(&tasks, &view.id).await;
    let approved = tasks.continue_task(ContinueTaskRequest {
        id: view.id.clone(),
        approve: Some(true),
        ..ContinueTaskRequest::default()
    });
    assert!(approved.ok);
    assert!(matches!(
        settle(&tasks, &view.id).await.status,
        TaskStatus::Done { .. }
    ));
    let requests = script.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    // The whole `if` (its own continuation is not recoverable from the
    // path) and the step after it both still run.
    assert_eq!(
        requests[2].flow.steps,
        [
            FlowStep::Action(FlowAction::If(IfStep {
                condition: "a draft is open".to_owned(),
                then: vec![FlowStep::Action(FlowAction::StopBefore(
                    "sending the email".to_owned()
                ))],
                otherwise: Vec::new(),
            })),
            FlowStep::Intent("finish another email message".to_owned()),
        ]
    );
}

#[tokio::test]
async fn an_approval_resume_spends_from_the_tasks_remaining_budget_not_a_fresh_one() {
    // Every scripted reply reports 3 actions spent (`finished_run`'s fixed
    // `actions: 3`), and the task's budget allows 10 in total.
    let (tasks, script) = controller(vec![
        gated("Send", "sending the email"),
        finished_run(FlowStopReason::Completed, vec![], &[], None),
        finished_run(FlowStopReason::Completed, vec![], &[], None),
    ]);
    let started = tasks.start(&StartTaskRequest {
        flow: Some(flow(mail_flow())),
        budget: tinycomputer_bus::agent::TaskBudget {
            max_actions: Some(10),
            ..tinycomputer_bus::agent::TaskBudget::default()
        },
        ..StartTaskRequest::default()
    });
    let view = started.data.unwrap();
    settle(&tasks, &view.id).await;
    let _ = tasks.continue_task(ContinueTaskRequest {
        id: view.id.clone(),
        approve: Some(true),
        ..ContinueTaskRequest::default()
    });
    assert!(matches!(
        settle(&tasks, &view.id).await.status,
        TaskStatus::Done { .. }
    ));
    let requests = script.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[0].max_actions, 10,
        "the first run gets the full budget"
    );
    assert_eq!(
        requests[1].max_actions, 7,
        "the approval's own run only gets what the first run did not spend"
    );
    assert_eq!(
        requests[2].max_actions, 4,
        "the rest gets only what neither earlier run spent, not a fresh 10"
    );
}

#[tokio::test]
async fn an_exhausted_time_budget_fails_the_task_before_a_run_starts() {
    let (tasks, script) = controller(vec![finished_run(
        FlowStopReason::Completed,
        vec![],
        &[],
        None,
    )]);
    let started = tasks.start(&StartTaskRequest {
        flow: Some(flow(json!({"app": "Mail", "steps": ["a"]}))),
        budget: tinycomputer_bus::agent::TaskBudget {
            max_elapsed_ms: Some(0),
            ..tinycomputer_bus::agent::TaskBudget::default()
        },
        ..StartTaskRequest::default()
    });
    let view = started.data.unwrap();
    let status = settle(&tasks, &view.id).await.status;
    let TaskStatus::Failed { reason, .. } = &status else {
        panic!("{status:?}");
    };
    assert!(reason.contains("time budget"));
    assert!(
        script.requests.lock().unwrap().is_empty(),
        "no run was ever started"
    );
    assert_eq!(*script.released.lock().unwrap(), [view.id]);
}

#[tokio::test]
async fn a_declined_action_cancels_and_a_payment_is_always_a_checkpoint() {
    let (tasks, _) = controller(vec![gated("Send", "sending the email")]);
    let view = start(&tasks, mail_flow(), &[]);
    settle(&tasks, &view.id).await;
    let declined = tasks
        .continue_task(ContinueTaskRequest {
            id: view.id.clone(),
            approve: Some(false),
            ..ContinueTaskRequest::default()
        })
        .data
        .unwrap();
    assert_eq!(declined.status, TaskStatus::Cancelled);

    let (tasks, script) = controller(vec![gated("Pay ₹6,840", "paying for the booking")]);
    let view = start(&tasks, mail_flow(), &[]);
    let stopped = settle(&tasks, &view.id).await;
    let TaskStatus::Checkpoint {
        continuable,
        reason,
        summary,
        ..
    } = &stopped.status
    else {
        panic!("{:?}", stopped.status);
    };
    assert!(!continuable);
    assert!(reason.contains("payment"));
    assert!(summary.contains("start a new email message"));
    assert!(stopped.status.is_final());
    // A final checkpoint's workspace is left open for a person to pay in, so
    // it is not released on its own; `CancelTask` must stay offered as the
    // only path to release it once they are done, or it would permanently
    // consume one of the browser's limited session slots.
    assert_eq!(stopped.next, ["CancelTask", "TaskReport"]);
    assert!(script.released.lock().unwrap().is_empty());
    let again = tasks.continue_task(ContinueTaskRequest {
        id: view.id.clone(),
        approve: Some(true),
        ..ContinueTaskRequest::default()
    });
    assert_eq!(code(&again), "NOT_WAITING");
    let cancelled = tasks.cancel(&view.id).data.unwrap();
    assert_eq!(
        cancelled.status, stopped.status,
        "cancelling a final checkpoint releases its workspace without changing its status"
    );
    assert_eq!(*script.released.lock().unwrap(), [view.id]);
}

#[tokio::test]
async fn filling_the_payment_form_waits_for_approval_to_pay() {
    let (tasks, script) = controller(vec![
        gated("Pay ₹6,840", "paying for the booking"),
        finished_run(FlowStopReason::Completed, vec![], &[], None),
        finished_run(FlowStopReason::Completed, vec![], &[], None),
    ]);
    let started = tasks.start(&StartTaskRequest {
        task: Some("book the cheapest flight to Srinagar and pay with my card".to_owned()),
        flow: Some(flow(mail_flow())),
        facts: BTreeMap::from([
            ("first name".to_owned(), "Asha".to_owned()),
            ("card number".to_owned(), "4111111111111111".to_owned()),
        ]),
        constraints: TaskConstraints {
            payment: PaymentMode::FillThenApprove,
            origins: vec!["https://.airline.test".to_owned()],
            ..TaskConstraints::default()
        },
        budget: tinycomputer_bus::agent::TaskBudget {
            votes: Some(7),
            strategy: Some(tinycomputer_bus::FlowStrategy::Wide),
            ..tinycomputer_bus::agent::TaskBudget::default()
        },
        ..StartTaskRequest::default()
    });
    let view = started.data.unwrap();
    let paused = settle(&tasks, &view.id).await;
    let TaskStatus::NeedsApproval { target, .. } = &paused.status else {
        panic!("{:?}", paused.status);
    };
    assert_eq!(target, "Pay ₹6,840");
    {
        let requests = script.requests.lock().unwrap();
        let request = &requests[0];
        assert_eq!(request.votes, 7);
        assert_eq!(request.strategy, tinycomputer_bus::FlowStrategy::Wide);
        assert_eq!(
            request.brief.goal,
            "book the cheapest flight to Srinagar and pay with my card"
        );
        assert_eq!(request.brief.details["first name"], "Asha");
        assert_eq!(request.brief.secrets, ["card number"]);
        assert!(
            !serde_json::to_string(&request.brief)
                .unwrap()
                .contains("4111")
        );
        assert!(
            request
                .brief
                .rules
                .iter()
                .any(|rule| rule.contains("Fill the payment form")),
            "{:?}",
            request.brief.rules
        );
    }
    let paid = tasks
        .continue_task(ContinueTaskRequest {
            id: view.id.clone(),
            approve: Some(true),
            ..ContinueTaskRequest::default()
        })
        .data
        .unwrap();
    assert_eq!(paid.status, TaskStatus::Running);
    settle(&tasks, &view.id).await;
    let requests = script.requests.lock().unwrap();
    assert!(
        requests[1].allow_destructive,
        "only the approval lets the pay control be pressed"
    );
}
