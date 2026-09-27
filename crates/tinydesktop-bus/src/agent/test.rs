//! Tests pinning the Agent interface's wire form.
//!
//! A model reads and writes these frames directly, so field names and tags
//! are the contract.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use serde_json::json;

use super::{
    AgentError, AgentResponse, AwaitTaskRequest, ContinueTaskRequest, InputField, InputKind,
    StartTaskRequest, SurfaceKind, TaskId, TaskStatus, TaskView,
};

#[test]
fn a_bare_task_takes_safe_defaults() {
    let request: StartTaskRequest =
        serde_json::from_value(json!({"task": "book a flight"})).unwrap();
    assert_eq!(request.task.as_deref(), Some("book a flight"));
    assert!(request.flow.is_none());
    assert!(!request.constraints.allow_destructive);
    assert!(!request.constraints.headed);
    assert!(request.constraints.surfaces.is_empty());
    assert!(!request.trace);
    let round_trip: StartTaskRequest =
        serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
    assert_eq!(round_trip, request);
}

#[test]
fn a_flow_and_constraints_are_accepted_as_written() {
    let request: StartTaskRequest = serde_json::from_value(json!({
        "flow": {"app": "Browser", "steps": [{"browse": "https://flights.test"}, "search for flights"]},
        "constraints": {
            "surfaces": ["browser", "desktop"],
            "origins": ["https://.flights.test"],
            "browser_endpoint": "http://127.0.0.1:9222",
            "headed": true
        },
        "budget": {"max_actions": 80}
    }))
    .unwrap();
    assert_eq!(request.flow.unwrap().steps.len(), 2);
    assert_eq!(
        request.constraints.surfaces,
        [SurfaceKind::Browser, SurfaceKind::Desktop]
    );
    assert_eq!(request.budget.max_actions, Some(80));
    assert_eq!(request.budget.max_model_calls, None);
}

#[test]
fn statuses_are_tagged_by_state() {
    let fields = TaskStatus::NeedsInput {
        fields: vec![InputField {
            name: "cabin".to_owned(),
            why: "the fare depends on it".to_owned(),
            kind: InputKind::Choice,
            options: vec!["economy".to_owned(), "business".to_owned()],
        }],
    };
    assert_eq!(
        serde_json::to_value(&fields).unwrap(),
        json!({"state": "needs_input", "fields": [{
            "name": "cabin", "why": "the fare depends on it", "kind": "choice",
            "options": ["economy", "business"]
        }]})
    );
    assert_eq!(
        serde_json::to_value(TaskStatus::Running).unwrap(),
        json!({"state": "running"})
    );
    let checkpoint = TaskStatus::Checkpoint {
        reason: "reached the payment page".to_owned(),
        location: "https://flights.test/pay".to_owned(),
        screenshot: None,
        summary: "IndiGo 6E-2135, ₹6,840, traveller details filled".to_owned(),
        continuable: false,
    };
    let wire = serde_json::to_value(&checkpoint).unwrap();
    assert_eq!(wire["state"], "checkpoint");
    assert!(wire.get("screenshot").is_none());
    assert_eq!(
        serde_json::from_value::<TaskStatus>(wire).unwrap(),
        checkpoint
    );
}

#[test]
fn only_settled_statuses_are_final() {
    let done = TaskStatus::Done {
        answer: "done".to_owned(),
        records: BTreeMap::new(),
    };
    let failed = TaskStatus::Failed {
        step: Some(2),
        reason: "no results".to_owned(),
        hint: "try another date".to_owned(),
        recoverable: true,
    };
    let payment = TaskStatus::Checkpoint {
        reason: String::new(),
        location: String::new(),
        screenshot: None,
        summary: String::new(),
        continuable: false,
    };
    let review = TaskStatus::Checkpoint {
        reason: String::new(),
        location: String::new(),
        screenshot: None,
        summary: String::new(),
        continuable: true,
    };
    for status in [done, failed, payment, TaskStatus::Cancelled] {
        assert!(status.is_final(), "{status:?}");
    }
    for status in [
        TaskStatus::Running,
        review,
        TaskStatus::NeedsHuman {
            reason: "solve the captcha".to_owned(),
            screenshot: None,
        },
        TaskStatus::NeedsPlan {
            guide: String::new(),
        },
        TaskStatus::NeedsApproval {
            action: "send the email".to_owned(),
            target: "Send".to_owned(),
            screenshot: None,
        },
    ] {
        assert!(!status.is_final(), "{status:?}");
    }
}

#[test]
fn replies_carry_data_or_an_actionable_error() {
    let view = TaskView {
        id: TaskId::new("t-1"),
        status: TaskStatus::Running,
        summary: "Searching for flights.".to_owned(),
        step: None,
        progress: 0.25,
        next: vec!["AwaitTask".to_owned()],
    };
    let ok = serde_json::to_value(AgentResponse::ok(view)).unwrap();
    assert_eq!(ok["ok"], true);
    assert!(ok.get("error").is_none());
    assert!(ok["data"].get("step").is_none());

    let failed: AgentResponse<TaskView> = AgentResponse::err(AgentError::new(
        "NO_SUCH_TASK",
        "task t-9 does not exist",
        "call ListTasks for current task ids",
        false,
    ));
    let wire = serde_json::to_value(&failed).unwrap();
    assert_eq!(wire["error"]["code"], "NO_SUCH_TASK");
    assert!(wire.get("data").is_none());
    assert_eq!(
        serde_json::from_value::<AgentResponse<TaskView>>(wire).unwrap(),
        failed
    );
}

#[test]
fn waiting_and_continuing_have_forgiving_defaults() {
    let wait: AwaitTaskRequest = serde_json::from_value(json!({"id": "t-1"})).unwrap();
    assert_eq!(wait.timeout_ms, 30_000);
    let resume: ContinueTaskRequest = serde_json::from_value(json!({
        "id": "t-1", "inputs": {"date of birth": "1990-04-02"}
    }))
    .unwrap();
    assert_eq!(resume.id.to_string(), "t-1");
    assert_eq!(resume.inputs["date of birth"], "1990-04-02");
    assert_eq!(resume.approve, None);
}
