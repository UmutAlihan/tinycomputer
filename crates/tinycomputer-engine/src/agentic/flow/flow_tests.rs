//! Tests for intent flows against a simulated mail app and a scripted Jev.
//!
//! `Sim` is a tiny stateful application: it shows an inbox with a New Message
//! button, opens a compose window on click or cmd+n, holds field values, and
//! records every press. `Oracle` answers Jev questions from the same state, the
//! way a well-behaved decision model would, and each test overrides only the
//! answers it is about.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use serde_json::{Value, json};
use tinycomputer_bus::{
    Deliberation, DesktopResponse, Flow, FlowLoop, FlowRunResult, FlowStopReason, GroundingHint,
    JevExchange, JevOperation, RunFlowRequest, StepOutcome, ValidateFlowRequest,
};
use tinyinference_decisions::{
    Answer, ChoiceAnswer, EvaluationFailure, EvaluationRequest, EvaluationResponse,
    EvaluationResult, NoulAnswer, Question, ScoreAnswer,
};

use super::{
    super::{Evaluator, JevRuntime},
    ask,
    backend::AgentBackend,
    decide::fit,
    enter, flow_guide, ground, memory, run_flow_with,
    steps::{
        self, already_chosen, already_holds, in_region, lists_more_than, looks_like_date, redacted,
    },
    validate, validate_flow,
    view::{Candidate, Depth, Screen},
    vote,
};

fn runtime(oracle: Oracle) -> JevRuntime {
    JevRuntime {
        client: Arc::new(oracle),
        configuration: tinycomputer_bus::JevConfiguration {
            provider: tinycomputer_bus::JevProvider::OpenRouter,
            model: "jev-latest".to_owned(),
            endpoint_url: None,
        },
        pending: Arc::default(),
        journal: crate::agentic::journal::Journal::default(),
    }
}

struct Run {
    result: FlowRunResult,
    app: App,
    requests: Vec<EvaluationRequest>,
}

async fn run_with(
    app: App,
    flow: Value,
    configure: impl FnOnce(&mut RunFlowRequest),
    hook: impl Fn(&str, &Question, &Sim) -> Option<Answer> + Send + Sync + 'static,
) -> Run {
    let oracle = Arc::new(Oracle {
        app: app.clone(),
        hook: Box::new(hook),
        requests: Mutex::new(Vec::new()),
        fail: false,
    });
    let runtime = JevRuntime {
        client: oracle.clone(),
        configuration: tinycomputer_bus::JevConfiguration {
            provider: tinycomputer_bus::JevProvider::OpenRouter,
            model: "jev-latest".to_owned(),
            endpoint_url: None,
        },
        pending: Arc::default(),
        journal: crate::agentic::journal::Journal::default(),
    };
    // One framing per decision, so every test that counts requests counts
    // decisions; voting has its own tests.
    let mut request = RunFlowRequest {
        flow: serde_json::from_value(flow).unwrap(),
        include_values: true,
        trace: true,
        votes: 1,
        ..RunFlowRequest::default()
    };
    configure(&mut request);
    let reply = run_flow_with(app.clone(), &runtime, request).await;
    assert!(reply.ok, "flow run failed: {:?}", reply.error);
    let requests = oracle.requests.lock().unwrap().clone();
    Run {
        result: serde_json::from_value(reply.data.unwrap()).unwrap(),
        app,
        requests,
    }
}

async fn run(app: App, flow: Value) -> Run {
    run_with(app, flow, |_| {}, |_, _, _| None).await
}

fn mail_flow() -> Value {
    json!({
        "app": "Mail",
        "vars": {"to": "sam@example.com"},
        "steps": [
            {"open": "Mail"},
            "start a new email message",
            {"enter": {
                "recipient": "${to}",
                "subject": "Moving Thursday's sync",
                "message body": "Hi Sam,\n\nCould we move it to Friday?\n\nAlex"
            }},
            {"verify": "the draft shows the recipient, subject and body"},
            {"stop_before": "sending the email"}
        ]
    })
}

fn outcomes(result: &FlowRunResult) -> Vec<(String, StepOutcome)> {
    result
        .steps
        .iter()
        .map(|step| (step.path.clone(), step.outcome))
        .collect()
}

fn choice_sizes(requests: &[EvaluationRequest]) -> Vec<usize> {
    requests
        .iter()
        .flat_map(|request| request.questions.values())
        .filter_map(|question| match question {
            Question::Choice(choice) => Some(choice.criteria.len()),
            _ => None,
        })
        .collect()
}
