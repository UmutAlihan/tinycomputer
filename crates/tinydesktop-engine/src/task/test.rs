//! Tests for the task controller over scripted flow runs.
//!
//! The runner hands back queued `RunFlow` replies and records every request,
//! so each pause, resume, and failure path is exercised without Jev or a
//! surface.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use serde_json::json;
use tinydesktop_bus::agent::{
    AgentResponse, AwaitTaskRequest, ContinueTaskRequest, InputKind, StartTaskRequest,
    TaskConstraints, TaskId, TaskStatus, TaskView,
};
use tinydesktop_bus::{
    DesktopError, DesktopResponse, Flow, FlowAction, FlowRunResult, FlowStep, FlowStopReason,
    JevMetrics, JevTarget, RunFlowRequest, StepOutcome, StepReport,
};

use super::interpret::app_at;
use super::{FlowFuture, FlowRunner, MAX_TASKS, Tasks, input_kind, next_calls};

/// Replies queued in order; a missing reply never resolves, like a flow
/// still running.
#[derive(Default)]
struct Script {
    replies: Mutex<VecDeque<DesktopResponse>>,
    requests: Mutex<Vec<RunFlowRequest>>,
}

impl FlowRunner for Script {
    fn run(&self, _constraints: &TaskConstraints, request: RunFlowRequest) -> FlowFuture {
        self.requests.lock().unwrap().push(request);
        let reply = self.replies.lock().unwrap().pop_front();
        Box::pin(async move {
            match reply {
                Some(reply) => reply,
                None => std::future::pending().await,
            }
        })
    }
}

fn controller(replies: Vec<DesktopResponse>) -> (Tasks, Arc<Script>) {
    let script = Arc::new(Script {
        replies: Mutex::new(replies.into()),
        requests: Mutex::default(),
    });
    (Tasks::new(script.clone()), script)
}

fn step(path: &str, kind: &str, text: &str, outcome: StepOutcome, note: &str) -> StepReport {
    StepReport {
        path: path.to_owned(),
        kind: kind.to_owned(),
        text: text.to_owned(),
        outcome,
        turns: 1,
        jev_calls: 1,
        actions: Vec::new(),
        loops: Vec::new(),
        confidence: None,
        note: note.to_owned(),
    }
}

fn finished_run(
    stop: FlowStopReason,
    steps: Vec<StepReport>,
    vars: &[(&str, &str)],
    pending: Option<&str>,
) -> DesktopResponse {
    let result = FlowRunResult {
        stop,
        steps,
        vars: vars
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        pending: pending.map(|name| JevTarget {
            ref_id: "e9".to_owned(),
            role: "button".to_owned(),
            name: Some(name.to_owned()),
        }),
        learned: Vec::new(),
        actions: 3,
        metrics: JevMetrics::default(),
        trace: Vec::new(),
    };
    DesktopResponse::ok("run-flow", serde_json::to_value(result).unwrap())
}

fn flow(value: serde_json::Value) -> Flow {
    serde_json::from_value(value).unwrap()
}

fn start(tasks: &Tasks, flow_value: serde_json::Value, facts: &[(&str, &str)]) -> TaskView {
    let reply = tasks.start(StartTaskRequest {
        flow: Some(flow(flow_value)),
        facts: facts
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        ..StartTaskRequest::default()
    });
    assert!(reply.ok, "{:?}", reply.error);
    reply.data.unwrap()
}

async fn settle(tasks: &Tasks, id: &TaskId) -> TaskView {
    tasks
        .await_task(AwaitTaskRequest {
            id: id.clone(),
            timeout_ms: 5_000,
        })
        .await
        .data
        .unwrap()
}

fn code<T>(reply: &AgentResponse<T>) -> &str {
    &reply.error.as_ref().expect("an error").code
}

#[tokio::test]
async fn a_finished_flow_is_done_with_its_reads_and_no_fact_values() {
    let (tasks, script) = controller(vec![finished_run(
        FlowStopReason::Completed,
        vec![
            step("1", "browse", "https://flights.test", StepOutcome::Done, ""),
            step("2", "enter", "email asha@example.com", StepOutcome::Done, ""),
        ],
        &[("email", "asha@example.com"), ("cheapest", "IndiGo ₹6,840 for asha@example.com")],
        None,
    )]);
    let view = start(
        &tasks,
        json!({"app": "browser", "steps": [
            {"browse": "https://flights.test"},
            {"enter": {"email": "${email}"}}
        ]}),
        &[("email", "asha@example.com")],
    );
    assert_eq!(view.status, TaskStatus::Running);
    assert_eq!(view.next, ["AwaitTask", "CancelTask"]);
    let done = settle(&tasks, &view.id).await;
    let TaskStatus::Done { answer, records } = &done.status else {
        panic!("{:?}", done.status);
    };
    assert!(answer.contains("cheapest: IndiGo ₹6,840 for ‹email›"), "{answer}");
    assert!(!done.summary.contains("asha@"));
    assert_eq!(records["cheapest"][0]["value"], "IndiGo ₹6,840 for asha@example.com");
    assert!(!records.contains_key("email"), "facts are not records");
    assert!((done.progress - 1.0).abs() < f32::EPSILON);
    assert_eq!(done.step.as_ref().unwrap().intent, "email ‹email›");
    assert_eq!(done.step.as_ref().unwrap().surface, "browser");
    assert_eq!(done.next, ["TaskReport"]);

    let request = &script.requests.lock().unwrap()[0];
    assert_eq!(request.vars["email"], "asha@example.com");
    assert!(!request.include_values, "field values never leave for Jev");
    assert!(!request.allow_destructive);
    assert_eq!((request.max_actions, request.max_model_calls), (120, 300));

    let report = tasks.report(&view.id).data.unwrap();
    assert_eq!(report.steps.len(), 2);
    assert!(report.flow.is_some());
    assert_eq!(tasks.list().data.unwrap()[0].id, view.id);
}

#[tokio::test]
async fn missing_values_are_asked_for_before_anything_runs() {
    let (tasks, script) = controller(vec![finished_run(FlowStopReason::Completed, vec![], &[], None)]);
    let view = start(
        &tasks,
        json!({"app": "browser", "steps": [
            {"enter": {"email": "${email}", "birth date": "${date of birth}", "phone": "${phone}"}},
            {"read": {"what": "the fare", "into": "fare"}},
            {"verify": "the fare ${fare} is shown for ${travellers}"}
        ]}),
        &[("phone", "+91 98765 43210")],
    );
    let TaskStatus::NeedsInput { fields } = &view.status else {
        panic!("{:?}", view.status);
    };
    let asked = fields
        .iter()
        .map(|field| (field.name.as_str(), field.kind))
        .collect::<Vec<_>>();
    assert_eq!(
        asked,
        [
            ("email", InputKind::Email),
            ("date of birth", InputKind::Date),
            ("travellers", InputKind::Number)
        ],
        "a value a read step defines is not asked for"
    );
    assert_eq!(view.next, ["ContinueTask", "CancelTask"]);
    assert!(script.requests.lock().unwrap().is_empty());

    let partial = tasks.continue_task(ContinueTaskRequest {
        id: view.id.clone(),
        inputs: BTreeMap::from([("email".to_owned(), "a@b.c".to_owned())]),
        ..ContinueTaskRequest::default()
    });
    let TaskStatus::NeedsInput { fields } = &partial.data.unwrap().status else {
        panic!("still missing values");
    };
    assert_eq!(fields.len(), 2);

    let card = tasks.continue_task(ContinueTaskRequest {
        id: view.id.clone(),
        inputs: BTreeMap::from([("card number".to_owned(), "4111".to_owned())]),
        ..ContinueTaskRequest::default()
    });
    assert_eq!(code(&card), "CARD_DATA_REFUSED");

    let complete = tasks.continue_task(ContinueTaskRequest {
        id: view.id.clone(),
        inputs: BTreeMap::from([
            ("date of birth".to_owned(), "1990-04-02".to_owned()),
            ("travellers".to_owned(), "1".to_owned()),
        ]),
        ..ContinueTaskRequest::default()
    });
    assert_eq!(complete.data.unwrap().status, TaskStatus::Running);
    assert!(matches!(
        settle(&tasks, &view.id).await.status,
        TaskStatus::Done { .. }
    ));
    let request = &script.requests.lock().unwrap()[0];
    assert_eq!(request.vars["email"], "a@b.c");
    assert_eq!(request.vars["phone"], "+91 98765 43210");
}

#[tokio::test]
async fn requests_that_cannot_start_are_refused_with_a_hint() {
    let (tasks, _) = controller(Vec::new());
    let card = tasks.start(StartTaskRequest {
        flow: Some(flow(json!({"app": "Mail", "steps": ["x"]}))),
        facts: BTreeMap::from([("notes".to_owned(), "4111 1111 1111 1111".to_owned())]),
        ..StartTaskRequest::default()
    });
    assert_eq!(code(&card), "CARD_DATA_REFUSED");
    let nothing = tasks.start(StartTaskRequest::default());
    assert_eq!(code(&nothing), "INVALID_REQUEST");
    let invalid = tasks.start(StartTaskRequest {
        flow: Some(flow(json!({"app": "", "steps": []}))),
        ..StartTaskRequest::default()
    });
    assert_eq!(code(&invalid), "INVALID_FLOW");
    assert!(!invalid.error.unwrap().hint.is_empty());

    let planless = tasks.start(StartTaskRequest {
        task: Some("book the cheapest flight to Srinagar".to_owned()),
        ..StartTaskRequest::default()
    });
    let view = planless.data.unwrap();
    assert!(matches!(view.status, TaskStatus::NeedsPlan { ref guide } if guide.contains("browse")));
    assert_eq!(view.next, ["StartTask"]);
    assert_eq!(code(&tasks.continue_task(ContinueTaskRequest {
        id: view.id,
        ..ContinueTaskRequest::default()
    })), "NOT_WAITING");
}

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
            step("2", "do", "start a new email message", StepOutcome::Done, ""),
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
    assert_eq!((action.as_str(), target.as_str()), ("sending the email", "Send"));
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
    assert!(matches!(settle(&tasks, &view.id).await.status, TaskStatus::Done { .. }));
    let requests = script.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[1].allow_destructive, "only the approved action may be performed");
    assert_eq!(requests[1].flow.app, "Mail", "resumes on the app the flow had opened");
    assert_eq!(
        requests[1].flow.steps,
        [FlowStep::Action(FlowAction::StopBefore("sending the email".to_owned()))]
    );
    assert!(!requests[2].allow_destructive);
    assert_eq!(requests[2].flow.steps.len(), 1);
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

    let (tasks, _) = controller(vec![gated("Pay ₹6,840", "paying for the booking")]);
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
    assert_eq!(stopped.next, ["TaskReport"]);
    let again = tasks.continue_task(ContinueTaskRequest {
        id: view.id,
        approve: Some(true),
        ..ContinueTaskRequest::default()
    });
    assert_eq!(code(&again), "NOT_WAITING");
}

#[tokio::test]
async fn failures_carry_the_step_the_reason_and_what_to_change() {
    let failure = |reply: DesktopResponse| async move {
        let (tasks, _) = controller(vec![reply]);
        let view = start(&tasks, json!({"app": "Mail", "steps": ["a", "b"]}), &[]);
        settle(&tasks, &view.id).await.status
    };
    let step_failed = failure(finished_run(
        FlowStopReason::StepFailed,
        vec![
            step("1", "do", "a", StepOutcome::Done, ""),
            step("2", "do", "b", StepOutcome::Failed, "no search field was found"),
        ],
        &[],
        None,
    ))
    .await;
    assert!(matches!(
        step_failed,
        TaskStatus::Failed { step: Some(1), ref reason, recoverable: true, .. }
            if reason == "no search field was found"
    ));
    for (stop, hint) in [
        (FlowStopReason::ActionBudget, "max_actions"),
        (FlowStopReason::ModelBudget, "max_model_calls"),
        (FlowStopReason::Invalid, "guide"),
    ] {
        let status = failure(finished_run(stop, vec![], &[], None)).await;
        let TaskStatus::Failed { hint: got, .. } = status else {
            panic!("{stop:?}");
        };
        assert!(got.contains(hint), "{stop:?}: {got}");
    }
    let mut error = DesktopError::new("JEV_NOT_CONFIGURED", "jev is not configured");
    error.suggestion = Some("send the module a jev config".to_owned());
    let unconfigured = failure(DesktopResponse::err("run-flow", error)).await;
    assert!(matches!(
        unconfigured,
        TaskStatus::Failed { ref hint, recoverable: false, .. } if hint == "send the module a jev config"
    ));
    let bare = failure(DesktopResponse::err(
        "run-flow",
        DesktopError::new("X", "broken"),
    ))
    .await;
    assert!(matches!(bare, TaskStatus::Failed { ref hint, .. } if hint.contains("Jev")));
    let unreadable = failure(DesktopResponse::ok("run-flow", json!({"nonsense": true}))).await;
    assert!(matches!(unreadable, TaskStatus::Failed { ref reason, .. } if reason.contains("unreadable")));
    let mut no_error = DesktopResponse::err("run-flow", DesktopError::new("X", "x"));
    no_error.error = None;
    let silent = failure(no_error).await;
    assert!(matches!(silent, TaskStatus::Failed { ref reason, .. } if reason == "the flow could not run"));
}

#[tokio::test]
async fn a_running_task_can_be_awaited_briefly_and_cancelled() {
    let (tasks, _) = controller(Vec::new());
    let view = start(&tasks, json!({"app": "Mail", "steps": ["a"]}), &[]);
    let waited = tasks
        .await_task(AwaitTaskRequest {
            id: view.id.clone(),
            timeout_ms: 10,
        })
        .await
        .data
        .unwrap();
    assert_eq!(waited.status, TaskStatus::Running);
    let cancelled = tasks.cancel(&view.id).data.unwrap();
    assert_eq!(cancelled.status, TaskStatus::Cancelled);
    assert_eq!(tasks.cancel(&view.id).data.unwrap().status, TaskStatus::Cancelled);
    assert_eq!(settle(&tasks, &view.id).await.status, TaskStatus::Cancelled);
}

#[tokio::test]
async fn unknown_tasks_are_named_as_such() {
    let (tasks, _) = controller(Vec::new());
    for id in ["t-99", "nonsense"] {
        let id = TaskId::new(id);
        assert_eq!(code(&tasks.cancel(&id)), "NO_SUCH_TASK");
        assert_eq!(code(&tasks.report(&id)), "NO_SUCH_TASK");
        assert_eq!(
            code(&tasks.continue_task(ContinueTaskRequest {
                id: id.clone(),
                ..ContinueTaskRequest::default()
            })),
            "NO_SUCH_TASK"
        );
        let waited = tasks
            .await_task(AwaitTaskRequest {
                id: id.clone(),
                timeout_ms: 1,
            })
            .await;
        assert_eq!(code(&waited), "NO_SUCH_TASK");
    }
}

#[tokio::test]
async fn the_store_is_bounded_and_drops_finished_tasks_first() {
    let (tasks, _) = controller(Vec::new());
    let waiting = json!({"app": "Mail", "steps": [{"verify": "${x} holds"}]});
    let first = start(&tasks, waiting.clone(), &[]);
    for _ in 1..MAX_TASKS {
        start(&tasks, waiting.clone(), &[]);
    }
    let full = tasks.start(StartTaskRequest {
        flow: Some(flow(waiting.clone())),
        ..StartTaskRequest::default()
    });
    assert_eq!(code(&full), "TOO_MANY_TASKS");
    assert!(tasks.cancel(&first.id).ok);
    let admitted = start(&tasks, waiting, &[]);
    assert_eq!(tasks.list().data.unwrap().len(), MAX_TASKS);
    assert_eq!(code(&tasks.report(&first.id)), "NO_SUCH_TASK");
    assert_eq!(tasks.list().data.unwrap()[0].id, admitted.id);
    assert!(format!("{tasks:?}").contains("Tasks"));
}

#[test]
fn input_kinds_and_next_calls_follow_the_status() {
    assert_eq!(input_kind("mobile"), InputKind::Phone);
    assert_eq!(input_kind("dob"), InputKind::Date);
    assert_eq!(input_kind("number of rooms"), InputKind::Number);
    assert_eq!(input_kind("passport"), InputKind::Text);
    assert_eq!(
        next_calls(&TaskStatus::NeedsHuman {
            reason: "captcha".to_owned(),
            screenshot: None
        }),
        ["ContinueTask", "CancelTask", "TaskReport"]
    );
    assert_eq!(
        next_calls(&TaskStatus::Checkpoint {
            reason: String::new(),
            location: String::new(),
            screenshot: None,
            summary: String::new(),
            continuable: true
        }),
        ["ContinueTask", "TaskReport"]
    );
    assert_eq!(
        next_calls(&TaskStatus::Failed {
            step: None,
            reason: String::new(),
            hint: String::new(),
            recoverable: true
        }),
        ["TaskReport", "StartTask"]
    );
}

#[test]
fn the_app_in_front_is_the_last_opened_before_a_step() {
    let steps = flow(json!({"app": "Notes", "steps": [
        {"open": "Mail"}, {"browse": "https://x.test"}, "a", "b"
    ]}));
    assert_eq!(app_at(&steps, 0), "Notes");
    assert_eq!(app_at(&steps, 1), "Mail");
    assert_eq!(app_at(&steps, 3), "browser");
    assert_eq!(app_at(&steps, 99), "browser");
}
