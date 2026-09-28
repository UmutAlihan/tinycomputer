//! Tests for the task controller over scripted flow runs.
//!
//! The runner hands back queued `RunFlow` replies and records every request,
//! so each pause, resume, and failure path is exercised without Jev or a
//! surface.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};

use serde_json::json;
use tinycomputer_bus::agent::{
    AgentResponse, AwaitTaskRequest, ContinueTaskRequest, InputKind, PaymentMode, StartTaskRequest,
    TaskConstraints, TaskId, TaskStatus, TaskView,
};
use tinycomputer_bus::{
    DesktopError, DesktopResponse, Flow, FlowAction, FlowRunResult, FlowStep, FlowStopReason,
    IfStep, JevMetrics, JevTarget, RunFlowRequest, StepOutcome, StepReport,
};

use super::interpret::app_at;
use super::{FlowFuture, FlowRunner, MAX_TASKS, Tasks, capabilities, input_kind, next_calls};

/// Replies queued in order; a missing reply never resolves, like a flow
/// still running.
#[derive(Default)]
struct Script {
    replies: Mutex<VecDeque<DesktopResponse>>,
    requests: Mutex<Vec<RunFlowRequest>>,
    /// What the task's surface shows when asked.
    screen: Mutex<Vec<String>>,
    /// Tasks let go of, in order.
    released: Mutex<Vec<TaskId>>,
}

impl FlowRunner for Script {
    fn run(
        &self,
        _task: &TaskId,
        _constraints: &TaskConstraints,
        request: RunFlowRequest,
    ) -> FlowFuture {
        self.requests.lock().unwrap().push(request);
        let reply = self.replies.lock().unwrap().pop_front();
        Box::pin(async move {
            match reply {
                Some(reply) => reply,
                None => std::future::pending().await,
            }
        })
    }

    fn visible_text(&self, _task: &TaskId) -> super::TextFuture {
        let texts = self.screen.lock().unwrap().clone();
        Box::pin(async move { texts })
    }

    fn release(&self, task: &TaskId) {
        self.released.lock().unwrap().push(task.clone());
    }
}

fn controller(replies: Vec<DesktopResponse>) -> (Tasks, Arc<Script>) {
    let script = Arc::new(Script {
        replies: Mutex::new(replies.into()),
        ..Script::default()
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
    let reply = tasks.start(&StartTaskRequest {
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
            step(
                "2",
                "enter",
                "email asha@example.com",
                StepOutcome::Done,
                "",
            ),
        ],
        &[
            ("email", "asha@example.com"),
            ("cheapest", "IndiGo ₹6,840 for asha@example.com"),
        ],
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
    assert!(
        answer.contains("cheapest: IndiGo ₹6,840 for ‹email›"),
        "{answer}"
    );
    assert!(!done.summary.contains("asha@"));
    assert_eq!(
        records["cheapest"][0]["value"],
        "IndiGo ₹6,840 for asha@example.com"
    );
    assert!(!records.contains_key("email"), "facts are not records");
    assert!((done.progress - 1.0).abs() < f32::EPSILON);
    assert_eq!(done.step.as_ref().unwrap().intent, "email ‹email›");
    assert_eq!(done.step.as_ref().unwrap().surface, "browser");
    assert_eq!(done.next, ["TaskReport"]);

    let request = &script.requests.lock().unwrap()[0];
    assert_eq!(request.vars["email"], "asha@example.com");
    assert!(
        request.include_values,
        "Jev reads what fields hold; the runtime masks secrets"
    );
    assert!(!request.allow_destructive);
    assert_eq!(
        (request.max_actions, request.max_model_calls, request.votes),
        (120, 6000, 7)
    );
    assert!(request.facts.is_empty(), "an email is shared, not secret");
    assert_eq!(request.brief.details["email"], "asha@example.com");
    assert!(
        request
            .brief
            .rules
            .iter()
            .any(|rule| rule.contains("Never pay")),
        "{:?}",
        request.brief.rules
    );

    let report = tasks.report(&view.id).data.unwrap();
    assert_eq!(report.steps.len(), 2);
    assert!(report.flow.is_some());
    assert_eq!(tasks.list().data.unwrap()[0].id, view.id);
}

#[tokio::test]
async fn missing_values_are_asked_for_before_anything_runs() {
    let (tasks, script) = controller(vec![finished_run(
        FlowStopReason::Completed,
        vec![],
        &[],
        None,
    )]);
    let view = start(
        &tasks,
        json!({"app": "browser", "steps": [
            {"enter": {
                "email": "${email}",
                "birth date": "${date of birth}",
                "phone": "${phone}",
                "traveller count": "${travellers}"
            }},
            {"read": {"what": "the fare", "into": "fare"}},
            {"verify": "the fare ${fare} is shown"}
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
            ("date of birth", InputKind::Date),
            ("email", InputKind::Email),
            ("travellers", InputKind::Number)
        ],
        "first-use order (json! sorts the slots), and never a value a read step defines"
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

    // A card number is taken, as a secret.
    let card = tasks.continue_task(ContinueTaskRequest {
        id: view.id.clone(),
        inputs: BTreeMap::from([("card number".to_owned(), "4111".to_owned())]),
        ..ContinueTaskRequest::default()
    });
    assert!(matches!(
        card.data.unwrap().status,
        TaskStatus::NeedsInput { .. }
    ));

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
    assert_eq!(request.vars["card number"], "4111");
    assert_eq!(request.facts, BTreeSet::from(["card number".to_owned()]));
    assert_eq!(request.brief.secrets, ["card number"]);
    assert!(!request.brief.details.contains_key("card number"));
    assert_eq!(request.brief.details["date of birth"], "1990-04-02");
}

#[tokio::test]
async fn a_value_supplied_for_a_missing_fact_still_fails_fast_if_it_leaks() {
    // `passport number` is not declared at `StartTask`, so the flow only
    // looks like it is missing a plain value; once `ContinueTask` supplies
    // it, it becomes a secret the same as one declared up front, and the
    // `verify` step that reads it is exactly as invalid as if it had been
    // declared from the start. This must fail before the task spawns.
    let (tasks, script) = controller(Vec::new());
    let view = start(
        &tasks,
        json!({"app": "Mail", "steps": [
            {"verify": "shows ${passport number}"}
        ]}),
        &[],
    );
    let TaskStatus::NeedsInput { fields } = &view.status else {
        panic!("{:?}", view.status);
    };
    assert_eq!(fields[0].name, "passport number");

    let supplied = tasks.continue_task(ContinueTaskRequest {
        id: view.id,
        inputs: BTreeMap::from([("passport number".to_owned(), "Z1234567".to_owned())]),
        ..ContinueTaskRequest::default()
    });
    assert_eq!(code(&supplied), "INVALID_FLOW");
    assert!(
        supplied.error.unwrap().message.contains("is a secret"),
        "a secret supplied to answer a missing-input prompt is still a secret"
    );
    assert!(
        script.requests.lock().unwrap().is_empty(),
        "the invalid flow must never be run"
    );
}

#[tokio::test]
async fn requests_that_cannot_start_are_refused_with_a_hint() {
    let (tasks, _) = controller(Vec::new());
    let misspelt = tasks.start(&StartTaskRequest {
        flow: Some(flow(json!({"app": "Mail", "steps": ["x"]}))),
        facts: BTreeMap::from([("passport no".to_owned(), "Z1234567".to_owned())]),
        secret_facts: vec!["passport number".to_owned()],
        ..StartTaskRequest::default()
    });
    assert_eq!(code(&misspelt), "UNKNOWN_SECRET");
    let anywhere = tasks.start(&StartTaskRequest {
        flow: Some(flow(json!({"app": "Mail", "steps": ["x"]}))),
        constraints: TaskConstraints {
            payment: PaymentMode::FillThenApprove,
            ..TaskConstraints::default()
        },
        ..StartTaskRequest::default()
    });
    assert_eq!(code(&anywhere), "ORIGINS_REQUIRED");
    let nothing = tasks.start(&StartTaskRequest::default());
    assert_eq!(code(&nothing), "INVALID_REQUEST");
    let invalid = tasks.start(&StartTaskRequest {
        flow: Some(flow(json!({"app": "", "steps": []}))),
        ..StartTaskRequest::default()
    });
    assert_eq!(code(&invalid), "INVALID_FLOW");
    assert!(!invalid.error.unwrap().hint.is_empty());

    let fact_leak = tasks.start(&StartTaskRequest {
        flow: Some(flow(json!({"app": "Mail", "steps": [
            {"verify": "shows ${email}"}
        ]}))),
        facts: BTreeMap::from([("email".to_owned(), "sam@example.com".to_owned())]),
        secret_facts: vec!["email".to_owned()],
        ..StartTaskRequest::default()
    });
    assert_eq!(code(&fact_leak), "INVALID_FLOW");
    assert!(
        fact_leak.error.unwrap().message.contains("is a secret"),
        "a secret referenced outside an enter step fails fast"
    );
    let shared = tasks.start(&StartTaskRequest {
        flow: Some(flow(json!({"app": "Mail", "steps": [
            {"verify": "shows ${email}"}
        ]}))),
        facts: BTreeMap::from([("email".to_owned(), "sam@example.com".to_owned())]),
        ..StartTaskRequest::default()
    });
    assert!(
        shared.ok,
        "a shared fact may be named in any step: {:?}",
        shared.error
    );

    let planless = tasks.start(&StartTaskRequest {
        task: Some("book the cheapest flight to Srinagar".to_owned()),
        ..StartTaskRequest::default()
    });
    let view = planless.data.unwrap();
    assert!(matches!(view.status, TaskStatus::NeedsPlan { ref guide } if guide.contains("browse")));
    assert_eq!(view.next, ["StartTask"]);
    assert_eq!(
        code(&tasks.continue_task(ContinueTaskRequest {
            id: view.id,
            ..ContinueTaskRequest::default()
        })),
        "NOT_WAITING"
    );
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
            step(
                "2",
                "do",
                "b",
                StepOutcome::Failed,
                "no search field was found",
            ),
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
    assert!(
        matches!(unreadable, TaskStatus::Failed { ref reason, .. } if reason.contains("unreadable"))
    );
    let mut no_error = DesktopResponse::err("run-flow", DesktopError::new("X", "x"));
    no_error.error = None;
    let silent = failure(no_error).await;
    assert!(
        matches!(silent, TaskStatus::Failed { ref reason, .. } if reason == "the flow could not run")
    );
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
    assert_eq!(
        tasks.cancel(&view.id).data.unwrap().status,
        TaskStatus::Cancelled
    );
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
    let full = tasks.start(&StartTaskRequest {
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

#[tokio::test]
async fn describe_documents_every_member_and_its_examples_really_work() {
    let described = capabilities(Vec::new(), true, false, false);
    let names = described
        .members
        .iter()
        .map(|member| member.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, tinycomputer_bus::agent::names::METHODS);
    let confidential = described
        .members
        .iter()
        .filter(|member| member.confidential)
        .map(|member| member.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(confidential, tinycomputer_bus::agent::names::CONFIDENTIAL);
    assert!(described.step_kinds.iter().any(|kind| kind == "browse"));
    assert!(!described.planner_configured);
    assert!(!described.rescue_configured);
    assert!(capabilities(Vec::new(), true, true, true).rescue_configured);

    let flight = &described.examples[0];
    assert_eq!(flight.member, "StartTask");
    let request: StartTaskRequest = serde_json::from_value(flight.request.clone()).unwrap();
    let (tasks, _) = controller(Vec::new());
    let view = tasks.start(&request).data.unwrap();
    let TaskStatus::NeedsInput { fields } = view.status else {
        panic!("the example leaves one fact for the caller to supply");
    };
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].name, "phone");
    assert_eq!(fields[0].kind, InputKind::Phone);
    for example in &described.examples[1..] {
        assert!(names.contains(&example.member.as_str()));
    }
}

/// A model that always answers with the same text.
struct Fixed(Result<String, String>);

impl crate::planner::LanguageModel for Fixed {
    fn complete(&self, _turns: &[crate::planner::Turn]) -> crate::planner::Completion {
        let answer = self.0.clone();
        Box::pin(async move { answer })
    }
}

fn planned(replies: Vec<DesktopResponse>, answer: Result<&str, &str>) -> (Tasks, Arc<Script>) {
    let (tasks, script) = controller(replies);
    let model = Arc::new(Fixed(answer.map(str::to_owned).map_err(str::to_owned)));
    (
        tasks.with_planner(crate::planner::Planner::new(model)),
        script,
    )
}

#[tokio::test]
async fn a_plain_language_task_is_planned_then_run() {
    let (tasks, script) = planned(
        vec![finished_run(FlowStopReason::Completed, vec![], &[], None)],
        Ok(r#"{"app": "Mail", "steps": [{"enter": {"recipient": "${email}"}}]}"#),
    );
    assert!(tasks.planner_configured());
    let started = tasks
        .start(&StartTaskRequest {
            task: Some("email Sam".to_owned()),
            facts: BTreeMap::from([("email".to_owned(), "sam@example.com".to_owned())]),
            ..StartTaskRequest::default()
        })
        .data
        .unwrap();
    assert_eq!(started.summary, "Planning the task.");
    assert!(matches!(
        settle(&tasks, &started.id).await.status,
        TaskStatus::Done { .. }
    ));
    let requests = script.requests.lock().unwrap();
    assert_eq!(requests[0].flow.app, "Mail");
    assert_eq!(requests[0].vars["email"], "sam@example.com");
    assert_eq!(
        tasks
            .report(&started.id)
            .data
            .unwrap()
            .flow
            .unwrap()
            .steps
            .len(),
        1
    );
}

#[tokio::test]
async fn a_plan_that_needs_values_asks_and_a_failed_plan_says_so() {
    let (tasks, script) = planned(
        vec![finished_run(FlowStopReason::Completed, vec![], &[], None)],
        Ok(r#"{"app": "browser", "steps": [{"enter": {"phone": "${phone}"}}]}"#),
    );
    let started = tasks
        .start(&StartTaskRequest {
            task: Some("fill my phone".to_owned()),
            ..StartTaskRequest::default()
        })
        .data
        .unwrap();
    let waiting = settle(&tasks, &started.id).await;
    assert!(
        matches!(waiting.status, TaskStatus::NeedsInput { ref fields } if fields[0].name == "phone")
    );
    assert!(
        tasks
            .continue_task(ContinueTaskRequest {
                id: started.id.clone(),
                inputs: BTreeMap::from([("phone".to_owned(), "+91 98765 43210".to_owned())]),
                ..ContinueTaskRequest::default()
            })
            .ok
    );
    assert!(matches!(
        settle(&tasks, &started.id).await.status,
        TaskStatus::Done { .. }
    ));
    assert_eq!(
        script.requests.lock().unwrap()[0].vars["phone"],
        "+91 98765 43210"
    );

    let (tasks, _) = planned(Vec::new(), Err("the model is down"));
    let started = tasks
        .start(&StartTaskRequest {
            task: Some("anything".to_owned()),
            ..StartTaskRequest::default()
        })
        .data
        .unwrap();
    assert!(matches!(
        settle(&tasks, &started.id).await.status,
        TaskStatus::Failed { ref reason, recoverable: true, .. } if reason == "the model is down"
    ));
}

#[tokio::test]
async fn plan_task_drafts_without_acting() {
    use tinycomputer_bus::agent::PlanTaskRequest;

    let request = PlanTaskRequest {
        task: "email Sam".to_owned(),
        ..PlanTaskRequest::default()
    };
    let (tasks, _) = controller(Vec::new());
    assert_eq!(code(&tasks.plan(&request).await), "PLANNER_NOT_CONFIGURED");
    let (tasks, script) = planned(
        Vec::new(),
        Ok(r#"{"app": "Mail", "steps": ["start a new email message"]}"#),
    );
    assert_eq!(tasks.plan(&request).await.data.unwrap().flow.app, "Mail");
    assert!(
        script.requests.lock().unwrap().is_empty(),
        "planning never runs anything"
    );
    let (tasks, _) = planned(Vec::new(), Err("down"));
    assert_eq!(code(&tasks.plan(&request).await), "PLAN_FAILED");
}

fn failed_at_step_two() -> DesktopResponse {
    finished_run(
        FlowStopReason::StepFailed,
        vec![
            step("1", "browse", "https://flights.test", StepOutcome::Done, ""),
            step(
                "2",
                "do",
                "search for flights",
                StepOutcome::Failed,
                "nothing to click",
            ),
        ],
        &[],
        None,
    )
}

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

#[tokio::test]
async fn extracted_rows_become_structured_records() {
    let (tasks, _) = controller(vec![finished_run(
        FlowStopReason::Completed,
        vec![],
        &[("flights", r#"[["IndiGo","₹6,840"],["Vistara","₹7,210"]]"#)],
        None,
    )]);
    let view = start(&tasks, json!({"app": "browser", "steps": ["a"]}), &[]);
    let TaskStatus::Done { records, .. } = settle(&tasks, &view.id).await.status else {
        panic!("done");
    };
    assert_eq!(records["flights"].len(), 2);
    assert_eq!(records["flights"][1]["field 2"], "₹7,210");
}

#[tokio::test]
async fn a_run_gets_the_callers_values_and_the_flow_keeps_its_own_definitions() {
    let (tasks, script) = controller(Vec::new());
    start(
        &tasks,
        json!({"app": "browser", "vars": {"first_name": "${first name}"}, "steps": [
            {"enter": {"first name": "${first_name}"}}
        ]}),
        &[("first name", "Asha")],
    );
    for _ in 0..50 {
        if !script.requests.lock().unwrap().is_empty() {
            break;
        }
        tokio::task::yield_now().await;
    }
    let request = &script.requests.lock().unwrap()[0];
    assert_eq!(request.vars["first name"], "Asha");
    assert!(
        !request.vars.contains_key("first_name"),
        "a definition passed as a caller value would shadow its expansion"
    );
    assert_eq!(request.flow.vars["first_name"], "${first name}");
}

mod rescue;
