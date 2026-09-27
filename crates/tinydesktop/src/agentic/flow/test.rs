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
    sync::{Arc, Mutex},
    time::Duration,
};

use serde_json::{Value, json};
use tinydesktop_bus::{
    DesktopResponse, Flow, FlowLoop, FlowRunResult, FlowStopReason, GroundingHint, JevOperation,
    RunFlowRequest, StepOutcome, ValidateFlowRequest,
};
use tinyjevclient::{
    Answer, ChoiceAnswer, EvaluationFailure, EvaluationRequest, EvaluationResponse,
    EvaluationResult, NoulAnswer, Question, ScoreAnswer,
};

use super::{
    super::{
        AgentBackend, Evaluator, JevRuntime,
        screen::{Candidate, Depth, Screen},
    },
    ask, enter, flow_guide, ground, memory, run_flow_with, validate, validate_flow,
};

// ---------------------------------------------------------------- simulator

/// Fixed behaviours a test gives the simulated app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Quirk {
    /// Set-value on the body is silently ignored, as in a rich-text editor.
    BodyIgnoresSetValue,
    /// Every extra row sits in one list instead of three.
    OneRegion,
    /// No action changes anything.
    Frozen,
    /// Observation fails.
    FailObserve,
    /// Launching fails.
    FailLaunch,
}

#[derive(Debug, Default)]
struct Sim {
    compose_open: bool,
    sent: bool,
    obstacle: bool,
    fields: BTreeMap<String, String>,
    presses: Vec<String>,
    clicks: Vec<String>,
    launched: Vec<String>,
    extra_buttons: usize,
    quirks: BTreeSet<Quirk>,
}

impl Sim {
    fn has(&self, quirk: Quirk) -> bool {
        self.quirks.contains(&quirk)
    }
}

#[derive(Clone, Default)]
struct App(Arc<Mutex<Sim>>);

impl App {
    fn with(configure: impl FnOnce(&mut Sim)) -> Self {
        let app = Self::default();
        configure(&mut app.0.lock().unwrap());
        app
    }

    fn quirky(quirk: Quirk) -> Self {
        Self::with(|sim| {
            sim.quirks.insert(quirk);
        })
    }

    fn sim(&self) -> std::sync::MutexGuard<'_, Sim> {
        self.0.lock().unwrap()
    }
}

fn node(name: &str, role: &str, actions: &[&str], path: &[&str], y: f64) -> Candidate {
    Candidate {
        ref_id: format!("@s:{name}"),
        role: role.to_owned(),
        name: Some(name.to_owned()),
        available_actions: actions.iter().map(|action| (*action).to_owned()).collect(),
        bounds: Some(json!({"x": 10.0, "y": y})),
        path: path.iter().map(|label| (*label).to_owned()).collect(),
        ..Candidate::default()
    }
}

impl App {
    fn screen(&self) -> Screen {
        let sim = self.sim();
        let window = if sim.compose_open {
            "New Message"
        } else {
            "Inbox"
        };
        let root = format!("window {window:?}");
        let mut candidates = Vec::new();
        if sim.compose_open {
            for (index, field) in ["To", "Subject"].iter().enumerate() {
                let mut field_node = node(
                    field,
                    "textfield",
                    &["SetValue"],
                    &[&root, "group \"Header\""],
                    100.0 + 30.0 * f64::from(u8::try_from(index).unwrap()),
                );
                field_node.value = sim.fields.get(*field).map(|value| json!(value));
                candidates.push(field_node);
            }
            let mut body = node("Body", "textarea", &["SetValue"], &[&root], 300.0);
            body.value = sim.fields.get("Body").map(|value| json!(value));
            candidates.push(body);
            candidates.push(node(
                "Send",
                "button",
                &["Click"],
                &[&root, "toolbar"],
                40.0,
            ));
        } else {
            candidates.push(node(
                "New Message",
                "button",
                &["Click"],
                &[&root, "toolbar"],
                40.0,
            ));
            candidates.push(node(
                "Archive",
                "button",
                &["Click"],
                &[&root, "toolbar"],
                40.0,
            ));
            for index in 0..sim.extra_buttons {
                let region = if sim.has(Quirk::OneRegion) {
                    "list \"Messages\"".to_owned()
                } else {
                    format!("list \"Region {}\"", index % 3)
                };
                let name = format!("Message {index}");
                candidates.push(node(
                    &name,
                    "row",
                    &["Click"],
                    &[&root, &region],
                    60.0 + f64::from(u32::try_from(index).unwrap()),
                ));
            }
        }
        let mut surface = "window".to_owned();
        if sim.obstacle {
            surface = "sheet".to_owned();
            candidates.push(node(
                "Delete Draft",
                "button",
                &["Click"],
                &["sheet"],
                500.0,
            ));
            candidates.push(node(
                "Keep Editing",
                "button",
                &["Click"],
                &["sheet"],
                500.0,
            ));
        }
        Screen {
            app: "Mail".to_owned(),
            window: Some(window.to_owned()),
            surface,
            root: None,
            candidates,
            context: vec![format!("{window} heading")],
            truncated: None,
        }
    }
}

impl AgentBackend for App {
    fn observe(
        &self,
        _app: &str,
        _root: Option<&str>,
        _depth: Depth,
    ) -> Result<Screen, Box<DesktopResponse>> {
        if self.sim().has(Quirk::FailObserve) {
            return Err(Box::new(DesktopResponse::err(
                "snapshot",
                tinydesktop_bus::DesktopError::new("APP_NOT_FOUND", "no such app"),
            )));
        }
        Ok(self.screen())
    }

    fn execute(
        &self,
        operation: JevOperation,
        target: Option<Candidate>,
        text: Option<String>,
    ) -> DesktopResponse {
        let mut sim = self.sim();
        if sim.has(Quirk::Frozen) {
            return DesktopResponse::ok("fake", json!({}));
        }
        let name = target
            .as_ref()
            .and_then(|target| target.name.clone())
            .unwrap_or_default();
        match operation {
            JevOperation::Click => {
                sim.clicks.push(name.clone());
                match name.as_str() {
                    "New Message" => sim.compose_open = true,
                    "Send" => sim.sent = true,
                    "Keep Editing" => sim.obstacle = false,
                    "Archive" => sim.compose_open = false,
                    _ => {}
                }
            }
            JevOperation::TypeText if !(name == "Body" && sim.has(Quirk::BodyIgnoresSetValue)) => {
                sim.fields.insert(name, text.unwrap_or_default());
            }
            _ => {}
        }
        DesktopResponse::ok("fake", json!({}))
    }

    fn read_value(&self, target: &Candidate) -> Option<String> {
        let name = target.name.clone().unwrap_or_default();
        Some(self.sim().fields.get(&name).cloned().unwrap_or_default())
    }

    fn paste(&self, _app: &str, target: &Candidate, text: &str) -> DesktopResponse {
        self.sim()
            .fields
            .insert(target.name.clone().unwrap_or_default(), text.to_owned());
        DesktopResponse::ok("paste", json!({}))
    }

    fn press(&self, _app: &str, combo: &str) -> DesktopResponse {
        let mut sim = self.sim();
        sim.presses.push(combo.to_owned());
        if sim.has(Quirk::Frozen) {
            return DesktopResponse::ok("press", json!({}));
        }
        match combo {
            "cmd+n" => sim.compose_open = true,
            "escape" => sim.obstacle = false,
            _ => {}
        }
        DesktopResponse::ok("press", json!({}))
    }

    fn launch(&self, app: &str) -> DesktopResponse {
        let mut sim = self.sim();
        sim.launched.push(app.to_owned());
        if sim.has(Quirk::FailLaunch) {
            return DesktopResponse::err(
                "launch",
                tinydesktop_bus::DesktopError::new("APP_NOT_FOUND", "no such app"),
            );
        }
        DesktopResponse::ok("launch", json!({}))
    }
}

// ------------------------------------------------------------------ oracle

type Hook = dyn Fn(&str, &Question, &Sim) -> Option<Answer> + Send + Sync;

struct Oracle {
    app: App,
    hook: Box<Hook>,
    requests: Mutex<Vec<EvaluationRequest>>,
    fail: bool,
}

impl Evaluator for Oracle {
    fn evaluate<'a>(
        &'a self,
        request: &'a EvaluationRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EvaluationResult, EvaluationFailure>> + Send + 'a>>
    {
        Box::pin(async move {
            request
                .validate()
                .expect("every request the flow builds is valid");
            self.requests.lock().unwrap().push(request.clone());
            if self.fail {
                return Err(EvaluationFailure {
                    error: tinyjevclient::Error::RateLimited,
                    attempts: 1,
                    latency: Duration::ZERO,
                });
            }
            let sim = self.app.sim();
            let answers = request
                .questions
                .iter()
                .map(|(id, question)| {
                    let answer = (self.hook)(id, question, &sim)
                        .unwrap_or_else(|| default_answer(id, question, &sim));
                    (id.clone(), answer)
                })
                .collect::<BTreeMap<_, _>>();
            Ok(EvaluationResult {
                response: EvaluationResponse {
                    model: "typesafe/jev-test".to_owned(),
                    answers,
                    usage: tinyjevclient::Usage::default(),
                },
                request_id: None,
                attempts: 1,
                latency: Duration::from_millis(1),
            })
        })
    }
}

fn text_of(question: &Question, field: &str) -> String {
    let instructions = match question {
        Question::Choice(choice) => &choice.instructions,
        Question::Score(score) => &score.instructions,
        Question::Noul(noul) => &noul.instructions,
    };
    instructions
        .get(field)
        .map(|value| {
            value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned)
        })
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn noul(probability: f64) -> Answer {
    Answer::Noul(NoulAnswer { noul: probability })
}

fn level(position: usize) -> Answer {
    Answer::Score(ScoreAnswer {
        score: 0.0,
        legend: BTreeMap::new(),
        probabilities: (0..5)
            .map(|index| (index.to_string(), if index == position { 1.0 } else { 0.0 }))
            .collect(),
        confidence: 1.0,
    })
}

fn pick(question: &Question, needle: &str, probability: f64) -> Answer {
    let Question::Choice(choice) = question else {
        panic!("pick needs a choice question");
    };
    let key = choice
        .criteria
        .iter()
        .find(|(key, description)| {
            *key == needle
                || description
                    .as_ref()
                    .is_some_and(|description| description.to_string().contains(needle))
        })
        .map_or_else(|| "none".to_owned(), |(key, _)| key.clone());
    let rest = (1.0 - probability) / f64::from(u32::try_from(choice.criteria.len()).unwrap());
    Answer::Choice(ChoiceAnswer {
        probabilities: choice
            .criteria
            .keys()
            .map(|option| {
                (
                    option.clone(),
                    if *option == key { probability } else { rest },
                )
            })
            .collect(),
        choice: key,
        confidence: 0.5,
    })
}

fn needle_for(purpose: &str) -> &'static str {
    if purpose.contains("send") {
        "Send"
    } else if purpose.contains("new email") || purpose.contains("editable fields") {
        "New Message"
    } else if purpose.contains("recipient") {
        "To"
    } else if purpose.contains("subject") {
        "Subject"
    } else if purpose.contains("body") {
        "Body"
    } else if purpose.contains("message 7") {
        "Message 7"
    } else {
        "Archive"
    }
}

fn default_answer(id: &str, question: &Question, sim: &Sim) -> Answer {
    match id {
        "done" => {
            let step = text_of(question, "step");
            let done = (step.contains("new email") || step.contains("editable fields"))
                && sim.compose_open;
            noul(if done { 0.95 } else { 0.05 })
        }
        "holds" => {
            let condition = text_of(question, "condition");
            let held = if condition.contains("has happened") {
                sim.sent
            } else if condition.contains("draft shows") {
                ["To", "Subject", "Body"].iter().all(|field| {
                    sim.fields
                        .get(*field)
                        .is_some_and(|value| !value.is_empty())
                })
            } else {
                sim.compose_open
            };
            noul(if held { 0.9 } else { 0.1 })
        }
        "progress" => level(2),
        "blocked" => noul(if sim.obstacle { 0.9 } else { 0.05 }),
        "move" => pick(question, "shortcut", 0.9),
        "shortcut" => pick(question, "new_item", 0.9),
        "confirm" => noul(0.9),
        "dismiss" => pick(question, "Keep Editing", 0.9),
        "region" => pick(question, "Region 1", 0.9),
        _ if id.starts_with("slot_") => {
            pick(question, needle_for(&text_of(question, "purpose")), 0.9)
        }
        _ => pick(question, needle_for(&text_of(question, "purpose")), 0.9),
    }
}

// ------------------------------------------------------------------ harness

fn runtime(oracle: Oracle) -> JevRuntime {
    JevRuntime {
        client: Arc::new(oracle),
        configuration: tinydesktop_bus::JevConfiguration {
            provider: tinydesktop_bus::JevProvider::OpenRouter,
            model: "jev-latest".to_owned(),
            endpoint_url: None,
        },
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
        configuration: tinydesktop_bus::JevConfiguration {
            provider: tinydesktop_bus::JevProvider::OpenRouter,
            model: "jev-latest".to_owned(),
            endpoint_url: None,
        },
    };
    let mut request = RunFlowRequest {
        flow: serde_json::from_value(flow).unwrap(),
        include_values: true,
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

// -------------------------------------------------------------------- tests

#[tokio::test]
async fn a_mail_compose_flow_fills_every_field_and_stops_in_front_of_send() {
    let app = App::quirky(Quirk::BodyIgnoresSetValue);
    let run = run(app, mail_flow()).await;

    assert_eq!(run.result.stop, FlowStopReason::StoppedBeforeDestructive);
    assert_eq!(
        outcomes(&run.result),
        [
            ("1".to_owned(), StepOutcome::Done),
            ("2".to_owned(), StepOutcome::Done),
            ("3".to_owned(), StepOutcome::Done),
            ("4".to_owned(), StepOutcome::Done),
            ("5".to_owned(), StepOutcome::Gated),
        ]
    );
    let sim = run.app.sim();
    assert!(!sim.sent, "a gated flow must never press Send");
    assert_eq!(sim.fields["To"], "sam@example.com");
    assert_eq!(
        sim.fields["Body"],
        "Hi Sam,\n\nCould we move it to Friday?\n\nAlex"
    );
    assert_eq!(sim.presses, ["cmd+n"]);
    assert_eq!(
        run.result.pending.as_ref().unwrap().name.as_deref(),
        Some("Send")
    );

    let enter = &run.result.steps[2];
    assert!(enter.loops.contains(&FlowLoop::Slots));
    let body = enter
        .actions
        .iter()
        .find(|action| action.action == "fill message body")
        .unwrap();
    assert_eq!(
        body.note, "via paste",
        "an ignored set-value falls back to paste"
    );
    assert_eq!(
        enter.actions.first().unwrap().action,
        "fill recipient",
        "fields fill top to bottom"
    );
    assert!(
        run.result
            .learned
            .iter()
            .any(|hint| hint.key == "recipient" && hint.name.as_deref() == Some("To"))
    );
    assert!(run.result.metrics.calls > 0 && run.result.actions >= 5);
    assert!(
        choice_sizes(&run.requests)
            .iter()
            .all(|size| *size <= ask::CAP + 1)
    );
}

#[tokio::test]
async fn an_allowed_destructive_step_is_performed_and_verified() {
    let run = run_with(
        App::default(),
        mail_flow(),
        |request| request.allow_destructive = true,
        |_, _, _| None,
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert!(run.app.sim().sent);
    assert_eq!(run.result.steps[4].outcome, StepOutcome::Done);
}

#[tokio::test]
async fn a_step_already_accomplished_is_skipped_without_acting() {
    let app = App::with(|sim| sim.compose_open = true);
    let run = run(
        app,
        json!({"app": "Mail", "steps": ["start a new email message"]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.result.steps[0].outcome, StepOutcome::AlreadyDone);
    assert!(run.app.sim().presses.is_empty() && run.app.sim().clicks.is_empty());
}

#[tokio::test]
async fn activating_a_control_learns_where_it_was() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |_| {},
        |id, question, _| (id == "move").then(|| pick(question, "activate", 0.9)),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().clicks, ["New Message"]);
    assert_eq!(run.result.learned[0].name.as_deref(), Some("New Message"));
}

#[tokio::test]
async fn a_remembered_element_is_confirmed_instead_of_searched_for() {
    let hint = GroundingHint {
        app: "Mail".to_owned(),
        key: "start a new email message".to_owned(),
        role: "button".to_owned(),
        name: Some("New Message".to_owned()),
        path: vec!["window \"Inbox\"".to_owned(), "toolbar".to_owned()],
    };
    let run = run_with(
        App::with(|sim| sim.extra_buttons = 60),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| request.memory = vec![hint],
        |id, question, _| (id == "move").then(|| pick(question, "activate", 0.9)),
    )
    .await;
    assert_eq!(run.app.sim().clicks, ["New Message"]);
    let step = &run.result.steps[0];
    assert!(step.loops.contains(&FlowLoop::Memory));
    assert!(
        !step.loops.contains(&FlowLoop::Narrowing),
        "memory skips the search"
    );
}

#[tokio::test]
async fn a_large_screen_is_narrowed_by_region_before_choosing() {
    let run = run_with(
        App::with(|sim| sim.extra_buttons = 60),
        json!({"app": "Mail", "steps": ["open message 7"]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "region" => Some(pick(question, "Region 1", 0.9)),
            "done" => Some(noul(if sim.clicks.is_empty() { 0.05 } else { 0.9 })),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().clicks, ["Message 7"]);
    assert!(run.result.steps[0].loops.contains(&FlowLoop::Narrowing));
    assert!(
        choice_sizes(&run.requests)
            .iter()
            .all(|size| *size <= ask::CAP + 1)
    );
}

#[tokio::test]
async fn one_crowded_region_falls_back_to_a_knockout() {
    let run = run_with(
        App::with(|sim| {
            sim.extra_buttons = 45;
            sim.quirks.insert(Quirk::OneRegion);
        }),
        json!({"app": "Mail", "steps": ["open message 7"]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "region" => Some(pick(question, "Messages", 0.9)),
            "done" => Some(noul(if sim.clicks.is_empty() { 0.05 } else { 0.9 })),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.app.sim().clicks, ["Message 7"]);
    assert!(
        run.requests
            .iter()
            .any(|request| request.questions.contains_key("group_0")),
        "the knockout asks one question per group"
    );
}

#[tokio::test]
async fn a_low_confidence_choice_is_used_only_when_the_re_ask_agrees() {
    let agreed = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |_| {},
        |id, question, _| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(pick(question, "New Message", 0.5)),
            _ => None,
        },
    )
    .await;
    assert_eq!(agreed.app.sim().clicks, ["New Message"]);
    let loops = &agreed.result.steps[0].loops;
    assert!(loops.contains(&FlowLoop::Consistency) && loops.contains(&FlowLoop::Corroboration));

    let disagreed = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| request.max_actions = 3,
        |id, question, _| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(pick(question, "New Message", 0.5)),
            "again" => Some(pick(question, "Archive", 0.8)),
            "confirm" => Some(noul(0.3)),
            _ => None,
        },
    )
    .await;
    assert!(
        disagreed.app.sim().clicks.is_empty(),
        "disagreement means no click"
    );
    assert_eq!(disagreed.result.stop, FlowStopReason::StepFailed);
}

#[tokio::test]
async fn an_obstacle_is_dismissed_with_a_safe_control_only() {
    let run = run(
        App::with(|sim| sim.obstacle = true),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert!(run.result.steps[0].loops.contains(&FlowLoop::Obstacles));
    assert_eq!(run.app.sim().clicks, ["Keep Editing"]);
    let dismiss = run
        .requests
        .iter()
        .find_map(|request| request.questions.get("dismiss"))
        .unwrap();
    assert!(
        !serde_json::to_string(dismiss)
            .unwrap()
            .contains("Delete Draft"),
        "an irreversible control is never offered to clear an obstacle"
    );

    let escaped = run_with(
        App::with(|sim| sim.obstacle = true),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |_| {},
        |id, question, _| (id == "dismiss").then(|| pick(question, "escape", 0.9)),
    )
    .await;
    assert!(escaped.app.sim().presses.contains(&"escape".to_owned()));
}

#[tokio::test]
async fn a_regression_is_undone_and_the_element_is_not_tried_again() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| request.max_actions = 6,
        |id, question, sim| match id {
            "move" => Some(pick(
                question,
                if sim.presses.contains(&"escape".to_owned()) {
                    "shortcut"
                } else {
                    "activate"
                },
                0.9,
            )),
            "target" => Some(pick(question, "Archive", 0.9)),
            "progress" => Some(level(if sim.clicks.is_empty() { 3 } else { 0 })),
            _ => None,
        },
    )
    .await;
    let sim = run.app.sim();
    assert_eq!(sim.clicks, ["Archive"]);
    assert!(sim.presses.contains(&"escape".to_owned()));
    assert!(run.result.steps[0].loops.contains(&FlowLoop::Undo));
    assert_eq!(run.result.stop, FlowStopReason::Completed);
}

#[tokio::test]
async fn actions_that_change_nothing_fail_the_step() {
    let run = run(
        App::quirky(Quirk::Frozen),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(run.result.steps[0].note.contains("changed nothing"));
}

#[tokio::test]
async fn an_irreversible_control_is_refused_inside_an_ordinary_step() {
    let run = run_with(
        App::with(|sim| sim.compose_open = true),
        json!({"app": "Mail", "steps": ["get rid of this draft"]}),
        |request| request.max_actions = 4,
        |id, question, _| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(pick(question, "Send", 0.95)),
            _ => None,
        },
    )
    .await;
    assert!(!run.app.sim().sent);
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
}

#[tokio::test]
async fn move_outcomes_cover_finished_stuck_wait_and_a_missing_shortcut() {
    let finished = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy up"]}),
        |_| {},
        |id, question, _| (id == "move").then(|| pick(question, "finished", 0.9)),
    )
    .await;
    assert_eq!(finished.result.stop, FlowStopReason::Completed);

    let stuck = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy up"]}),
        |_| {},
        |id, question, _| (id == "move").then(|| pick(question, "stuck", 0.9)),
    )
    .await;
    assert_eq!(stuck.result.stop, FlowStopReason::StepFailed);

    let waited = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy up"]}),
        |request| request.max_actions = 2,
        |id, question, _| match id {
            "move" => Some(pick(question, "wait", 0.9)),
            "shortcut" => Some(pick(question, "none", 0.9)),
            _ => None,
        },
    )
    .await;
    assert_eq!(waited.result.stop, FlowStopReason::ActionBudget);

    let no_shortcut = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy up"]}),
        |request| request.max_model_calls = 4,
        |id, question, _| (id == "shortcut").then(|| pick(question, "none", 0.9)),
    )
    .await;
    assert_eq!(no_shortcut.result.stop, FlowStopReason::ModelBudget);
    assert!(no_shortcut.app.sim().presses.is_empty());
}

#[tokio::test]
async fn return_is_refused_while_a_dialog_is_showing() {
    let run = run_with(
        App::with(|sim| sim.obstacle = true),
        json!({"app": "Mail", "steps": ["confirm the name"]}),
        |request| {
            request.disabled_loops = vec![FlowLoop::Obstacles];
            request.max_model_calls = 6;
        },
        |id, question, _| match id {
            "shortcut" => Some(pick(question, "confirm", 0.9)),
            _ => None,
        },
    )
    .await;
    assert!(!run.app.sim().presses.contains(&"return".to_owned()));
    let confirmed = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["confirm the name"]}),
        |request| request.max_actions = 1,
        |id, question, _| match id {
            "shortcut" => Some(pick(question, "confirm", 0.9)),
            _ => None,
        },
    )
    .await;
    assert_eq!(confirmed.app.sim().presses, ["return"]);
}

#[tokio::test]
async fn disabled_loops_are_not_asked_and_the_move_falls_back_to_pressing() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| {
            request.disabled_loops = vec![
                FlowLoop::Moves,
                FlowLoop::Progress,
                FlowLoop::Obstacles,
                FlowLoop::Consistency,
                FlowLoop::Corroboration,
                FlowLoop::Undo,
                FlowLoop::Memory,
                FlowLoop::Narrowing,
                FlowLoop::Slots,
            ];
        },
        |_, _, _| None,
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().clicks, ["New Message"]);
    let asked = run
        .requests
        .iter()
        .flat_map(|request| request.questions.keys().cloned())
        .collect::<BTreeSet<_>>();
    assert!(!asked.contains("move") && !asked.contains("progress") && !asked.contains("blocked"));

    let blind = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| {
            request.disabled_loops = vec![
                FlowLoop::Moves,
                FlowLoop::Progress,
                FlowLoop::Obstacles,
                FlowLoop::Completion,
            ];
        },
        |_, _, _| None,
    )
    .await;
    assert_eq!(
        blind.result.stop,
        FlowStopReason::StepFailed,
        "without the completion judge a step cannot recognise its own success"
    );
    assert_eq!(blind.app.sim().clicks, ["New Message"]);
}

#[tokio::test]
async fn control_steps_branch_repeat_read_and_wait() {
    let run = run_with(
        App::default(),
        json!({
            "app": "Mail",
            "steps": [
                {"if": {"condition": "a compose window is open",
                        "then": [{"verify": "never taken"}],
                        "else": ["start a new email message"]}},
                {"repeat_until": {"condition": "a compose window is open",
                                  "steps": ["start a new email message"], "max": 2}},
                {"wait_for": "a compose window is open"},
                {"read": {"what": "the window heading", "into": "heading"}},
                {"enter": {"subject": "About ${heading}"}}
            ]
        }),
        |_| {},
        |id, question, _| (id == "source").then(|| pick(question, "New Message heading", 0.9)),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.result.vars["heading"], "New Message heading");
    assert_eq!(run.app.sim().fields["Subject"], "About New Message heading");
    let paths = run
        .result
        .steps
        .iter()
        .map(|step| step.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["1", "1.1", "2", "3", "4", "5"]);
}

#[tokio::test]
async fn a_repeat_that_never_holds_and_a_failing_verify_fail_the_flow() {
    let repeat = run(
        App::quirky(Quirk::Frozen),
        json!({"app": "Mail", "steps": [
            {"repeat_until": {"condition": "a compose window is open",
                              "steps": [{"wait_for": "nothing"}], "max": 1}}
        ]}),
    )
    .await;
    assert_eq!(repeat.result.stop, FlowStopReason::StepFailed);

    let verify = run(
        App::default(),
        json!({"app": "Mail", "steps": [{"verify": "a compose window is open"}]}),
    )
    .await;
    assert_eq!(verify.result.stop, FlowStopReason::StepFailed);
    assert!(verify.result.steps[0].note.contains("does not hold"));

    let branch_then = run(
        App::with(|sim| sim.compose_open = true),
        json!({"app": "Mail", "steps": [
            {"if": {"condition": "a compose window is open", "then": [{"verify": "a compose window is open"}]}},
            {"repeat_until": {"condition": "a compose window is open", "steps": ["x"]}}
        ]}),
    )
    .await;
    assert_eq!(branch_then.result.stop, FlowStopReason::Completed);
}

#[tokio::test]
async fn choose_reveals_the_list_first_when_the_option_is_not_visible() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": [{"choose": {"what": "the message list", "option": "Message 7"}}]}),
        |_| {},
        |id, question, sim| match id {
            "target" if sim.extra_buttons == 0 => Some(pick(question, "none", 0.9)),
            "move" => Some(pick(question, "activate", 0.9)),
            "done" => Some(noul(0.9)),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(run.result.steps[0].note.contains("was not found"));

    let found = run_with(
        App::with(|sim| sim.extra_buttons = 9),
        json!({"app": "Mail", "steps": [{"choose": {"what": "the message list", "option": "Message 7"}}]}),
        |_| {},
        |_, _, _| None,
    )
    .await;
    assert_eq!(found.result.stop, FlowStopReason::Completed);
    assert_eq!(found.app.sim().clicks, ["Message 7"]);
}

#[tokio::test]
async fn enter_reveals_fields_and_fails_for_a_slot_with_no_field() {
    let revealed = run(
        App::default(),
        json!({"app": "Mail", "steps": [{"enter": {"subject": "Hi"}}]}),
    )
    .await;
    assert_eq!(revealed.result.stop, FlowStopReason::Completed);
    assert_eq!(revealed.app.sim().fields["Subject"], "Hi");

    let missing = run_with(
        App::with(|sim| sim.compose_open = true),
        json!({"app": "Mail", "steps": [{"enter": {"shoe size": "11"}}]}),
        |request| request.max_actions = 6,
        |id, question, _| {
            (id.starts_with("slot_") || id == "target").then(|| pick(question, "none", 0.9))
        },
    )
    .await;
    assert_eq!(missing.result.stop, FlowStopReason::StepFailed);

    let remembered = run_with(
        App::with(|sim| sim.compose_open = true),
        json!({"app": "Mail", "steps": [{"enter": {"subject": "Remembered"}}]}),
        |request| {
            request.memory = vec![GroundingHint {
                app: "Mail".to_owned(),
                key: "subject".to_owned(),
                role: "textfield".to_owned(),
                name: Some("Subject".to_owned()),
                path: vec![
                    "window \"New Message\"".to_owned(),
                    "group \"Header\"".to_owned(),
                ],
            }];
        },
        |_, _, _| None,
    )
    .await;
    assert_eq!(remembered.app.sim().fields["Subject"], "Remembered");
    assert!(
        remembered
            .requests
            .iter()
            .all(|request| !request.questions.contains_key("slot_0")),
        "a remembered field needs no slot question"
    );
}

#[tokio::test]
async fn budgets_invalid_flows_and_provider_failures_stop_cleanly() {
    let actions = run_with(
        App::default(),
        mail_flow(),
        |request| request.max_actions = 1,
        |_, _, _| None,
    )
    .await;
    assert_eq!(actions.result.stop, FlowStopReason::ActionBudget);

    let calls = run_with(
        App::default(),
        mail_flow(),
        |request| request.max_model_calls = 1,
        |_, _, _| None,
    )
    .await;
    assert_eq!(calls.result.stop, FlowStopReason::ModelBudget);

    let unopened = run(
        App::quirky(Quirk::FailLaunch),
        json!({"app": "Mail", "steps": [{"open": "Nope"}]}),
    )
    .await;
    assert_eq!(unopened.result.stop, FlowStopReason::StepFailed);
    assert!(unopened.result.steps[0].note.contains("APP_NOT_FOUND"));

    let unreadable = run(
        App::quirky(Quirk::FailObserve),
        json!({"app": "Mail", "steps": ["anything"]}),
    )
    .await;
    assert!(
        unreadable.result.steps[0]
            .note
            .contains("could not be read")
    );

    let failing = Oracle {
        app: App::default(),
        hook: Box::new(|_, _, _| None),
        requests: Mutex::new(Vec::new()),
        fail: true,
    };
    let reply = run_flow_with(
        App::default(),
        &runtime(failing),
        RunFlowRequest {
            flow: serde_json::from_value(json!({"app": "Mail", "steps": ["x"]})).unwrap(),
            ..RunFlowRequest::default()
        },
    )
    .await;
    assert_eq!(reply.error.unwrap().code, "JEV_RATE_LIMITED");

    let invalid = run_flow_with(
        App::default(),
        &runtime(Oracle {
            app: App::default(),
            hook: Box::new(|_, _, _| None),
            requests: Mutex::new(Vec::new()),
            fail: false,
        }),
        RunFlowRequest::default(),
    )
    .await;
    assert_eq!(invalid.error.unwrap().code, "FLOW_INVALID");
}

#[test]
fn validation_reports_every_problem_by_step() {
    let reply = validate_flow(&ValidateFlowRequest {
        flow: json!({
            "app": " ",
            "steps": [
                "",
                {"enter": {}},
                {"enter": {"x": "${missing}"}},
                {"read": {"what": "w", "into": "bad name"}},
                {"repeat_until": {"condition": "c", "steps": [], "max": 0}},
                {"if": {"condition": "c"}},
                {"choose": {"what": "", "option": "o"}},
                {"stop_before": "${also_missing}"},
                {"if": {"condition": "c", "then": [{"if": {"condition": "c", "then": [
                    {"if": {"condition": "c", "then": [{"if": {"condition": "c", "then": [
                        {"if": {"condition": "c", "then": ["deep"]}}]}}]}}]}}]}}
            ]
        }),
    });
    let validation: tinydesktop_bus::FlowValidation =
        serde_json::from_value(reply.data.unwrap()).unwrap();
    assert!(!validation.valid);
    let errors = validation.errors.join("\n");
    for expected in [
        "`app` must name",
        "step 1: the intent must not be empty",
        "step 2: `enter` needs at least one slot",
        "step 3: `${missing}` is not defined",
        "step 4: `into` must be a variable name",
        "step 5: `max` must be between 1 and 20",
        "step 5: `repeat_until` needs at least one step",
        "step 6: `if` needs a `then` or an `else` branch",
        "step 7: `what` must not be empty",
        "step 8: `${also_missing}` is not defined",
        "nests deeper than 4 levels",
    ] {
        assert!(
            errors.contains(expected),
            "missing {expected:?} in:\n{errors}"
        );
    }

    let malformed = validate::validate(
        &json!({"app": "Mail", "steps": [{"click": "x"}]}),
        &BTreeSet::new(),
    );
    assert!(malformed.0.is_none());
    assert!(malformed.1.errors[0].contains("not well formed"));

    let empty = validate::check(&Flow::default(), &BTreeSet::new());
    assert!(
        empty
            .errors
            .iter()
            .any(|error| error.contains("at least one step"))
    );

    let huge = Flow {
        app: "Mail".to_owned(),
        vars: BTreeMap::new(),
        steps: vec![tinydesktop_bus::FlowStep::Intent("x".to_owned()); 101],
    };
    assert!(validate::check(&huge, &BTreeSet::new()).errors[0].contains("at most 100"));

    let ok = validate::validate(&mail_flow(), &BTreeSet::new());
    assert!(ok.0.is_some() && ok.1.valid && ok.1.steps == 5);
    let with_runtime_var = validate::check(
        &serde_json::from_value(json!({"app": "Mail", "steps": [{"open": "${app}"}]})).unwrap(),
        &BTreeSet::from(["app".to_owned()]),
    );
    assert!(with_runtime_var.valid);
}

#[test]
fn text_helpers_substitute_reference_and_normalize() {
    let vars = BTreeMap::from([("to".to_owned(), "sam".to_owned())]);
    assert_eq!(
        validate::substitute("hi ${to}, ${other}", &vars),
        "hi sam, ${other}"
    );
    assert_eq!(
        validate::references("${a} and ${b} and ${unclosed"),
        ["a", "b"]
    );
    assert_eq!(
        validate::normalize("  The Message-Body! "),
        "the message body"
    );
    assert_eq!(validate::step_path("", 0), "1");
    assert_eq!(validate::step_path("4", 1), "4.2");
}

#[test]
fn guide_and_answer_helpers_behave() {
    let guide = flow_guide();
    assert!(
        guide.data.unwrap()["guide"]
            .as_str()
            .unwrap()
            .contains("# Writing a desktop flow")
    );
    assert_eq!(ask::lettered(28)[..3], ["A", "B", "C"]);
    assert_eq!(ask::lettered(28)[26..], ["AA", "AB"]);
    let answers = BTreeMap::from([
        ("progress".to_owned(), level(4)),
        (
            "flat".to_owned(),
            Answer::Score(ScoreAnswer {
                score: 0.0,
                legend: BTreeMap::new(),
                probabilities: BTreeMap::from([("0".to_owned(), 1.0)]),
                confidence: 1.0,
            }),
        ),
        ("yes".to_owned(), noul(0.7)),
    ]);
    assert!((ask::level(&answers, "progress").unwrap() - 1.0).abs() < 1e-9);
    assert!(ask::level(&answers, "flat").is_none());
    assert!(ask::level(&answers, "yes").is_none());
    assert!(ask::probability(&answers, "progress").is_none());
    assert!(ask::chosen(&answers, "yes").is_none());
    assert!(ask::Questions::default().is_empty());
}

#[test]
fn regions_split_at_the_first_level_that_divides_and_merge_the_tail() {
    let pool = (0..30)
        .map(|index| {
            node(
                &format!("b{index}"),
                "button",
                &["Click"],
                &["window", &format!("r{index}")],
                0.0,
            )
        })
        .collect::<Vec<_>>();
    let (level, regions) = ground::split(&pool, 0).unwrap();
    assert_eq!(level, 1);
    assert_eq!(regions.len(), ask::CAP);
    assert_eq!(regions.last().unwrap().0, "everything else");
    assert_eq!(regions.last().unwrap().1.len(), 30 - (ask::CAP - 1));
    let flat = vec![
        node("a", "button", &[], &[], 0.0),
        node("b", "button", &[], &[], 0.0),
    ];
    assert!(ground::split(&flat, 0).is_none());
}

#[test]
fn memory_matches_by_role_name_and_path_tail_and_replaces_old_hints() {
    let field = node(
        "Subject",
        "textfield",
        &["SetValue"],
        &["app", "window", "group"],
        0.0,
    );
    let hint = memory::remember("Mail", "The Subject", &field);
    assert_eq!(hint.key, "the subject");
    let pool = [field.clone()];
    assert!(memory::recall(std::slice::from_ref(&hint), "Mail", "the subject!", &pool).is_some());
    assert!(memory::recall(std::slice::from_ref(&hint), "Notes", "the subject", &pool).is_none());
    let moved = node("Subject", "textfield", &[], &["elsewhere"], 0.0);
    assert!(memory::recall(std::slice::from_ref(&hint), "Mail", "the subject", &[moved]).is_none());
    let mut hints = vec![hint.clone()];
    memory::learn(&mut hints, hint);
    assert_eq!(hints.len(), 1);
}

#[test]
fn editable_fields_are_found_by_action_or_role_in_reading_order() {
    let mut screen = App::with(|sim| sim.compose_open = true).screen();
    screen.candidates.push(Candidate {
        role: "combobox".to_owned(),
        name: Some("From".to_owned()),
        ..Candidate::default()
    });
    let names = enter::editable(&screen)
        .into_iter()
        .map(|field| field.name.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names, ["To", "Subject", "Body", "From"]);
}
