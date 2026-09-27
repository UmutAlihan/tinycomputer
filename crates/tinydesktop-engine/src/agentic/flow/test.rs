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
use tinyinference_decisions::{
    Answer, ChoiceAnswer, EvaluationFailure, EvaluationRequest, EvaluationResponse,
    EvaluationResult, NoulAnswer, Question, ScoreAnswer,
};

use super::{
    super::{Evaluator, JevRuntime},
    ask,
    backend::AgentBackend,
    enter, flow_guide, ground, memory, run_flow_with, validate, validate_flow,
    view::{Candidate, Depth, Screen},
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
    /// The surface has no addresses, as a desktop application has none.
    NoAddresses,
    /// The compose fields sit in a subtree the budgeted snapshot cut short.
    HiddenEditor,
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
    navigated: Vec<String>,
    /// Result cards: (airline, price, departure), shown as a list.
    results: Vec<(&'static str, &'static str, &'static str)>,
    /// Refs of the result cards' "Select" buttons clicked, in order.
    picked: Vec<String>,
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

/// The simulator's result list: each card's text as ref-less nodes, and its
/// "Select" button among `candidates`, under an ordinal-labelled list item.
fn result_cards(sim: &Sim, root: &str, candidates: &mut Vec<Candidate>) -> Vec<Candidate> {
    let mut text_nodes = Vec::new();
    for (index, (airline, price, departure)) in sim.results.iter().enumerate() {
        let card = format!("listitem #{}", index + 1);
        let path = vec![root.to_owned(), "list \"Results\"".to_owned(), card];
        let order = 1_000 + index * 10;
        for (offset, text) in [airline, price, departure].into_iter().enumerate() {
            text_nodes.push(Candidate {
                role: "text".to_owned(),
                value: Some(json!(text)),
                path: path.clone(),
                order: order + offset,
                ..Candidate::default()
            });
        }
        candidates.push(Candidate {
            ref_id: format!("@s:select-{}", index + 1),
            role: "button".to_owned(),
            name: Some("Select".to_owned()),
            available_actions: vec!["Click".to_owned()],
            path,
            order: order + 5,
            ..Candidate::default()
        });
    }
    text_nodes
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
        let text_nodes = result_cards(&sim, &root, &mut candidates);
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
            candidates,
            context: vec![format!("{window} heading")],
            unexplored: Vec::new(),
            text_nodes,
        }
    }
}

impl AgentBackend for App {
    fn observe(
        &self,
        _app: &str,
        root: Option<&str>,
        _depth: Depth,
    ) -> Result<Screen, Box<DesktopResponse>> {
        if self.sim().has(Quirk::FailObserve) {
            return Err(Box::new(DesktopResponse::err(
                "snapshot",
                tinydesktop_bus::DesktopError::new("APP_NOT_FOUND", "no such app"),
            )));
        }
        let mut screen = self.screen();
        if self.sim().has(Quirk::HiddenEditor) && self.sim().compose_open {
            let (fields, rest): (Vec<_>, Vec<_>) =
                screen.candidates.into_iter().partition(|candidate| {
                    candidate
                        .available_actions
                        .iter()
                        .any(|action| action == "SetValue")
                });
            if root == Some("@s:editor") {
                screen.candidates = fields;
            } else {
                screen.candidates = rest;
                screen.unexplored = vec!["@s:editor".to_owned()];
            }
        }
        Ok(screen)
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
                if let Some(reference) = target
                    .as_ref()
                    .map(|target| target.ref_id.clone())
                    .filter(|reference| reference.starts_with("@s:select-"))
                {
                    sim.picked.push(reference);
                }
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

    fn navigate(&self, url: &str) -> DesktopResponse {
        let mut sim = self.sim();
        if sim.has(Quirk::NoAddresses) {
            return DesktopResponse::err(
                "navigate",
                tinydesktop_bus::DesktopError::new("ACTION_NOT_SUPPORTED", "no addresses"),
            );
        }
        sim.navigated.push(url.to_owned());
        DesktopResponse::ok("navigate", json!({"url": url, "title": "Flights"}))
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
                    error: Box::new(tinyinference_decisions::Error::RateLimited),
                    attempts: 1,
                    latency: Duration::ZERO,
                });
            }
            let sim = self.app.sim();
            let answers = request
                .questions
                .iter()
                .map(|(id, question)| {
                    let answer = self.answer(request, id, question, &sim);
                    (id.clone(), answer)
                })
                .collect::<BTreeMap<_, _>>();
            Ok(EvaluationResult {
                response: EvaluationResponse {
                    model: "typesafe/jev-test".to_owned(),
                    answers,
                    usage: tinyinference_decisions::Usage::default(),
                },
                request_id: None,
                attempts: 1,
                latency: Duration::from_millis(1),
            })
        })
    }
}

impl Oracle {
    /// The hooked or default answer; a negated question is answered as the
    /// inverse of its positive twin, so hooks only ever name the positive one.
    fn answer(
        &self,
        request: &EvaluationRequest,
        id: &str,
        question: &Question,
        sim: &Sim,
    ) -> Answer {
        let twin = match id {
            "not_done" => Some("done"),
            "negated" | "coverage" => Some("holds"),
            _ => None,
        };
        if let Some(twin) = twin
            && let Some(positive) = request.questions.get(twin)
            && let Answer::Noul(answer) = self.answer(request, twin, positive, sim)
        {
            return if id == "coverage" {
                level(if answer.noul >= 0.5 { 4 } else { 0 })
            } else {
                noul(1.0 - answer.noul)
            };
        }
        if let Some(hooked) = (self.hook)(id, question, sim) {
            return hooked;
        }
        // Unhooked progress follows the completion answer, so a test that
        // scripts only "done" gets a consistent pair.
        if id == "progress"
            && let Some(done) = request.questions.get("done")
            && let Answer::Noul(answer) = self.answer(request, "done", done, sim)
        {
            return level(if answer.noul >= 0.5 { 4 } else { 2 });
        }
        default_answer(id, question, sim)
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
        pending: Arc::default(),
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
        pending: Arc::default(),
    };
    let mut request = RunFlowRequest {
        flow: serde_json::from_value(flow).unwrap(),
        include_values: true,
        trace: true,
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
    assert_eq!(
        run.result.trace.len(),
        usize::try_from(run.result.metrics.calls).unwrap()
    );
    assert_eq!(run.result.trace[0].step, "2");
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
    let run = run_with(
        app,
        json!({"app": "Mail", "steps": ["show the compose window"]}),
        |_| {},
        |id, _, sim| (id == "done").then(|| noul(if sim.compose_open { 0.95 } else { 0.05 })),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.result.steps[0].outcome, StepOutcome::AlreadyDone);
    assert!(run.app.sim().presses.is_empty() && run.app.sim().clicks.is_empty());
}

#[tokio::test]
async fn something_new_is_never_taken_to_exist_before_acting() {
    // An open draft is someone's own; "start a new email" must make another.
    let app = App::with(|sim| sim.compose_open = true);
    let run = run_with(
        app,
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |_| {},
        |id, question, sim| match id {
            "move" if sim.presses.is_empty() => Some(pick(question, "finished", 0.9)),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.result.steps[0].outcome, StepOutcome::Done);
    assert_eq!(run.app.sim().presses, ["cmd+n"]);
    assert!(super::act::creates_new("Create a folder"));
    assert!(!super::act::creates_new("open the inbox"));
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

/// Answers that press "Keep Editing" and never judge the step done, so only
/// the screen can end it.
fn press_keep_editing(id: &str, question: &Question, _: &Sim) -> Option<Answer> {
    match id {
        "done" | "blocked" => Some(noul(0.05)),
        "move" => Some(pick(question, "activate", 0.9)),
        _ if id == "target" || id == "region" || id.starts_with("group_") => {
            Some(pick(question, "Keep Editing", 0.9))
        }
        _ => None,
    }
}

#[tokio::test]
async fn pressing_the_named_control_that_closes_an_overlay_ends_the_step() {
    for step in [
        "close the dialog by keeping editing",
        "dismiss the save prompt",
    ] {
        let run = run_with(
            App::with(|sim| sim.obstacle = true),
            json!({"app": "Mail", "steps": [step]}),
            |_| {},
            press_keep_editing,
        )
        .await;
        assert_eq!(run.result.stop, FlowStopReason::Completed, "{step}");
        assert_eq!(run.app.sim().clicks, ["Keep Editing"], "{step}");
        assert!(
            run.result.steps[0].note.contains("closed"),
            "{}",
            run.result.steps[0].note
        );
    }
    let unrelated = run_with(
        App::with(|sim| sim.obstacle = true),
        json!({"app": "Mail", "steps": ["archive the message"]}),
        |request| request.max_actions = 1,
        press_keep_editing,
    )
    .await;
    assert_ne!(
        unrelated.result.stop,
        FlowStopReason::Completed,
        "closing an overlay the step never mentions does not finish it"
    );
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
            "progress" => Some(level(if sim.compose_open {
                4
            } else if sim.clicks.is_empty() {
                3
            } else {
                0
            })),
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
    assert!(
        run.result.steps[0].actions.is_empty(),
        "a refused destructive click must never be recorded as an action the step took"
    );
    assert_eq!(
        run.result.steps[0].turns, 1,
        "a refused destructive click must fail the step immediately, not after it has been \
         mistaken for a no-op action and stalled out"
    );
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
async fn a_control_the_flows_own_stop_before_names_is_refused_in_an_ordinary_step() {
    // "Archive" is not on the generic denylist, but this flow already plans
    // to stop in front of it later; an ordinary step must not press it first.
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": [
            "tidy up the inbox",
            {"stop_before": "archive the conversation"}
        ]}),
        |request| request.max_actions = 4,
        |id, question, _| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(pick(question, "Archive", 0.95)),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(
        run.app.sim().clicks.is_empty(),
        "a control the flow's own stop_before names must never be clicked early"
    );
}

#[tokio::test]
async fn an_unrecognized_move_is_skipped_rather_than_clicked() {
    // A malformed or prompt-injected answer must never fall through to
    // `activate`'s default Click branch; only `activate`, `expand`, and
    // `scroll` may ground and act.
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy up"]}),
        |request| request.max_actions = 4,
        |id, _, _| {
            (id == "move").then(|| {
                Answer::Choice(ChoiceAnswer {
                    choice: "delete_everything".to_owned(),
                    probabilities: BTreeMap::from([("delete_everything".to_owned(), 0.9)]),
                    confidence: 0.9,
                })
            })
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(
        run.app.sim().clicks.is_empty(),
        "an unrecognized move must never ground and click a control"
    );
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
        // 2: one for the implicit launch, one for the press itself.
        |request| request.max_actions = 2,
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
async fn a_read_target_beyond_the_source_cap_is_still_found_by_paging() {
    // 72 candidates ("New Message", "Archive", and 70 message rows) exceed
    // `ask::MAX_READ_SOURCES` (60); a target past that cutoff must still be
    // reachable a page at a time rather than permanently dropped.
    let run = run_with(
        App::with(|sim| sim.extra_buttons = 70),
        json!({"app": "Mail", "steps": [
            {"read": {"what": "the row for message 65", "into": "row"}}
        ]}),
        |_| {},
        |id, question, _| (id == "source").then(|| pick(question, "Message 65", 0.9)),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.result.vars["row"], "Message 65");
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
async fn a_repeat_conditions_trace_is_attributed_to_the_repeat_step_not_its_last_child() {
    // Round 0 runs its child ("start a new email message", path "1.r1.1"),
    // which opens the compose window. Round 1's condition check must then be
    // traced to "1", the repeat_until step itself, not left tagged with the
    // path of the child that last ran.
    let run = run(
        App::default(),
        json!({"app": "Mail", "steps": [
            {"repeat_until": {"condition": "a compose window is open",
                              "steps": ["start a new email message"], "max": 2}}
        ]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(
        run.result
            .trace
            .last()
            .map(|exchange| exchange.step.as_str()),
        Some("1"),
        "the condition check that ended the loop belongs to the repeat_until step"
    );
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
async fn choose_never_clicks_an_irreversible_option() {
    // "Send" is clickable and matches the requested option by name, but it is
    // irreversible; `choose` must fail the step through the usual `stop_before`
    // path rather than pressing it directly.
    let app = App::with(|sim| sim.compose_open = true);
    let run = run_with(
        app,
        json!({"app": "Mail", "steps": [{"choose": {"what": "the toolbar", "option": "Send"}}]}),
        |_| {},
        |_, _, _| None,
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(
        !run.app.sim().sent,
        "choose must never press an irreversible control"
    );
    assert!(!run.app.sim().clicks.contains(&"Send".to_owned()));
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
async fn entered_values_are_never_previewed_in_a_slot_matching_question() {
    // A slot's text can be a password, token, or private message; the model
    // only needs to know which field to type it into, not a preview of it.
    let run = run_with(
        App::with(|sim| sim.compose_open = true),
        json!({"app": "Mail", "steps": [{"enter": {"shoe size": "hunter2 super secret token"}}]}),
        |request| request.max_actions = 6,
        |id, question, _| {
            (id.starts_with("slot_") || id == "target").then(|| pick(question, "none", 0.9))
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(
        run.requests.iter().all(|request| request
            .questions
            .values()
            .all(|question| !text_of(question, "purpose").contains("hunter2"))),
        "the entered value must never reach a slot-matching question"
    );
}

#[tokio::test]
async fn the_implicit_launch_is_charged_to_the_action_budget() {
    // A run with no actions left must stop before touching the desktop at
    // all, and the launch it would otherwise perform for free must not be
    // missing from the reported action count on a run that does proceed.
    let starved = run_with(
        App::default(),
        mail_flow(),
        |request| request.max_actions = 0,
        |_, _, _| None,
    )
    .await;
    assert_eq!(starved.result.stop, FlowStopReason::ActionBudget);
    assert_eq!(starved.result.actions, 0);
    assert!(
        starved.app.sim().launched.is_empty(),
        "a starved run must never launch the application"
    );

    let launched = run(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
    )
    .await;
    assert_eq!(launched.app.sim().launched, ["Mail"]);
    assert!(
        launched.result.actions >= 1,
        "the implicit launch must count toward the reported actions"
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
fn validation_tracks_variables_along_execution_order() {
    let used_before_read = validate::check(
        &serde_json::from_value(json!({
            "app": "Mail",
            "steps": [
                {"verify": "shows ${name}"},
                {"read": {"what": "the name", "into": "name"}}
            ]
        }))
        .unwrap(),
        &BTreeSet::new(),
    );
    assert!(
        used_before_read
            .errors
            .iter()
            .any(|error| error.contains("${name}") && error.contains("not defined")),
        "a read later in the flow must not define its variable for an earlier step"
    );

    let after_read = validate::check(
        &serde_json::from_value(json!({
            "app": "Mail",
            "steps": [
                {"read": {"what": "the name", "into": "name"}},
                {"verify": "shows ${name}"}
            ]
        }))
        .unwrap(),
        &BTreeSet::new(),
    );
    assert!(after_read.valid);

    let only_in_untaken_branch = validate::check(
        &serde_json::from_value(json!({
            "app": "Mail",
            "steps": [
                {"if": {"condition": "c", "then": [
                    {"read": {"what": "the name", "into": "name"}}
                ]}},
                {"verify": "shows ${name}"}
            ]
        }))
        .unwrap(),
        &BTreeSet::new(),
    );
    assert!(
        only_in_untaken_branch
            .errors
            .iter()
            .any(|error| error.contains("${name}") && error.contains("not defined")),
        "a read defined only inside one `if` branch must not survive it"
    );

    let only_in_repeat = validate::check(
        &serde_json::from_value(json!({
            "app": "Mail",
            "steps": [
                {"repeat_until": {"condition": "c", "steps": [
                    {"read": {"what": "the name", "into": "name"}}
                ]}},
                {"verify": "shows ${name}"}
            ]
        }))
        .unwrap(),
        &BTreeSet::new(),
    );
    assert!(
        only_in_repeat
            .errors
            .iter()
            .any(|error| error.contains("${name}") && error.contains("not defined")),
        "a `repeat_until` body can run zero times, so its reads must not survive it"
    );
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
    assert_eq!(
        validate::substitute(
            "${a}",
            &BTreeMap::from([
                ("a".to_owned(), "${b}".to_owned()),
                ("b".to_owned(), "leaked".to_owned()),
            ]),
        ),
        "${b}",
        "a substituted value must not be rescanned for further references"
    );
    assert_eq!(
        validate::substitute("trailing ${unclosed", &BTreeMap::new()),
        "trailing ${unclosed"
    );
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
    screen.candidates.push(Candidate {
        role: "webarea".to_owned(),
        name: Some("message body".to_owned()),
        available_actions: vec!["SetFocus".to_owned()],
        ..Candidate::default()
    });
    let names = enter::editable(&screen)
        .into_iter()
        .map(|field| field.name.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names, ["To", "Subject", "Body", "From", "message body"]);
}

#[test]
fn a_rich_text_area_reports_the_text_inside_it_as_its_contents() {
    // The token label and the rich-text body lines are ref-less, as they are
    // in a real snapshot, so they live in `text_nodes` rather than
    // `candidates`; `order` places them in the document position they would
    // really occupy.
    let mut screen = App::with(|sim| {
        sim.compose_open = true;
        sim.fields.insert("Subject".to_owned(), "Hello".to_owned());
    })
    .screen();
    for (index, candidate) in screen.candidates.iter_mut().enumerate() {
        candidate.order = index * 10;
    }
    let body = Candidate {
        role: "webarea".to_owned(),
        name: Some("message body".to_owned()),
        available_actions: vec!["SetFocus".to_owned()],
        order: screen.candidates.len() * 10,
        ..Candidate::default()
    };
    let area = super::view::label(&body);
    screen.candidates.push(body);
    for (offset, line) in ["Hi Sam,", "See you Friday."].iter().enumerate() {
        screen.text_nodes.push(Candidate {
            role: "statictext".to_owned(),
            value: Some(json!(line)),
            path: vec![area.clone()],
            order: screen.candidates.len() * 10 + offset + 1,
            ..Candidate::default()
        });
    }
    // The "To" field turned into a token; its accessible name sits in a
    // ref-less static text node immediately after it, as it does for real.
    screen.candidates[0].value = Some(json!("\u{fffc}"));
    screen.text_nodes.push(Candidate {
        role: "statictext".to_owned(),
        name: Some("sam@example.com".to_owned()),
        order: 1,
        ..Candidate::default()
    });
    let state = ask::state(&screen, "check the draft", &[], true);
    let fields = state["field_contents"]["untrusted_accessibility_data"]
        .as_array()
        .unwrap();
    assert!(fields.contains(&json!({"field": "textfield \"Subject\"", "holds": "Hello"})));
    assert!(fields.contains(&json!({"field": "textfield \"To\"", "holds": "sam@example.com"})));
    assert!(fields.contains(&json!({"field": area, "holds": "Hi Sam,\nSee you Friday."})));
    assert!(
        ask::state(&screen, "x", &[], false)
            .get("field_contents")
            .is_none()
    );
    // The body's text must never appear in `visible_text`, which every
    // request shares regardless of `include_values`; only the gated
    // `field_contents` above may carry it.
    for include_values in [false, true] {
        let visible_text =
            ask::state(&screen, "x", &[], include_values)["visible_text"].to_string();
        assert!(!visible_text.contains("Hi Sam,"));
        assert!(!visible_text.contains("sam@example.com"));
    }
}

#[test]
fn a_named_control_with_a_numeric_value_reads_as_its_name() {
    let radio = |value: &str| Candidate {
        role: "radiobutton".to_owned(),
        name: Some("Dark".to_owned()),
        value: Some(json!(value)),
        ..Candidate::default()
    };
    assert_eq!(super::steps::readable(&radio("1")).as_deref(), Some("Dark"));
    assert_eq!(
        super::steps::readable(&radio("Night")).as_deref(),
        Some("Night")
    );
    let blank = Candidate {
        role: "group".to_owned(),
        name: Some(" ".to_owned()),
        ..Candidate::default()
    };
    assert!(super::steps::readable(&blank).is_none());
}

#[tokio::test]
async fn fields_in_a_truncated_subtree_are_found_by_exploring_it() {
    let run = run(
        App::with(|sim| {
            sim.compose_open = true;
            sim.quirks.insert(Quirk::HiddenEditor);
        }),
        json!({"app": "Mail", "steps": [{"enter": {"subject": "Found it"}}]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().fields["Subject"], "Found it");
}

#[tokio::test]
async fn an_app_that_never_shows_a_window_is_reported_as_opened_but_unreadable() {
    let run = run(
        App::quirky(Quirk::FailObserve),
        json!({"app": "Mail", "steps": [{"open": "Mail"}]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert!(run.result.steps[0].note.contains("no readable window yet"));
}

#[tokio::test]
async fn browse_opens_the_address_and_moves_the_flow_onto_the_page() {
    let run = run(
        App::default(),
        json!({
            "app": "Mail",
            "vars": {"to": "Srinagar"},
            "steps": [{"browse": "https://flights.test/to/${to}"}]
        }),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().launched, ["Mail", "browser"]);
    assert_eq!(
        run.app.sim().navigated,
        ["https://flights.test/to/Srinagar"]
    );
    assert_eq!(
        run.result.steps[0].note,
        "https://flights.test/to/Srinagar is open (Flights)"
    );
}

#[tokio::test]
async fn browse_fails_the_flow_where_there_is_no_browser_or_no_page() {
    for (quirk, code) in [
        (Quirk::FailLaunch, "APP_NOT_FOUND"),
        (Quirk::NoAddresses, "ACTION_NOT_SUPPORTED"),
    ] {
        let run = run(
            App::quirky(quirk),
            json!({"app": "browser", "steps": [{"browse": "https://flights.test"}]}),
        )
        .await;
        assert_eq!(run.result.stop, FlowStopReason::StepFailed, "{quirk:?}");
        assert!(
            run.result.steps[0].note.contains(code),
            "{}",
            run.result.steps[0].note
        );
    }
    let unreadable = run(
        App::quirky(Quirk::FailObserve),
        json!({"app": "browser", "steps": [{"browse": "https://flights.test"}]}),
    )
    .await;
    assert!(
        unreadable.result.steps[0]
            .note
            .contains("no readable page yet")
    );
}

fn flights() -> App {
    App::with(|sim| {
        sim.results = vec![
            ("IndiGo 6E-2135", "₹6,840", "6:45 PM"),
            ("Vistara UK-707", "₹7,210", "09:10"),
            ("Air India AI-825", "₹8,050", "05:30"),
        ];
    })
}

#[tokio::test]
async fn pick_ranks_a_measurable_criterion_exactly_and_opens_the_winner() {
    for (by, winner, airline) in [
        ("lowest price", "@s:select-1", "IndiGo"),
        ("earliest departure", "@s:select-3", "Air India"),
    ] {
        let run = run(
            flights(),
            json!({"app": "Mail", "steps": [
                {"pick": {"from": "the flight results", "by": by, "into": "flight"}},
                {"verify": "the page for ${flight} is open"}
            ]}),
        )
        .await;
        assert_eq!(run.app.sim().picked, [winner], "{by}");
        assert!(run.result.vars["flight"].starts_with(airline), "{by}");
        assert!(
            run.result.steps[0].note.contains("ranked"),
            "{}",
            run.result.steps[0].note
        );
        assert!(
            !run.requests
                .iter()
                .any(|request| request.questions.contains_key("record")),
            "a measurable criterion needs no judgement"
        );
    }
}

#[tokio::test]
async fn pick_asks_jev_when_the_criterion_needs_judgement() {
    let run = run_with(
        flights(),
        json!({"app": "Mail", "steps": [
            {"pick": {"from": "the flight results", "by": "the most comfortable airline"}}
        ]}),
        |_| {},
        |id, question, _| (id == "record").then(|| pick(question, "Vistara", 0.9)),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().picked, ["@s:select-2"]);
    assert!(run.result.steps[0].note.contains("judged"));
    assert!(
        !run.result.vars.contains_key("flight"),
        "no into, no variable"
    );

    let undecided = run_with(
        flights(),
        json!({"app": "Mail", "steps": [
            {"pick": {"from": "the flight results", "by": "the nicest"}}
        ]}),
        |_| {},
        |id, question, _| (id == "record").then(|| pick(question, "no such airline", 0.9)),
    )
    .await;
    assert_eq!(undecided.result.stop, FlowStopReason::StepFailed);
    assert!(undecided.result.steps[0].note.contains("clearly meets"));
}

#[tokio::test]
async fn pick_fails_where_no_list_is_showing() {
    let run = run(
        App::default(),
        json!({"app": "Mail", "steps": [
            {"pick": {"from": "the flight results", "by": "lowest price"}}
        ]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(
        run.result.steps[0]
            .note
            .contains("no list of the flight results")
    );
}

#[test]
fn pick_validates_its_fields_and_defines_its_variable() {
    let check = |flow: serde_json::Value| {
        super::validate::check(&serde_json::from_value(flow).unwrap(), &BTreeSet::new()).errors
    };
    assert!(
        check(json!({"app": "Mail", "steps": [
            {"pick": {"from": "results", "by": "cheapest", "into": "flight"}},
            {"verify": "${flight} is shown"}
        ]}))
        .is_empty()
    );
    let errors = check(json!({"app": "Mail", "steps": [
        {"pick": {"from": "", "by": " ", "into": "not a name"}}
    ]}));
    assert_eq!(errors.len(), 3, "{errors:?}");
}

#[tokio::test]
async fn extract_stores_every_item_of_the_list() {
    let run = run(
        flights(),
        json!({"app": "Mail", "steps": [
            {"extract": {"what": "the flight results", "into": "flights"}}
        ]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    let rows: Vec<Vec<String>> = serde_json::from_str(&run.result.vars["flights"]).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][..2], ["IndiGo 6E-2135", "₹6,840"]);
    assert!(run.app.sim().picked.is_empty(), "extracting opens nothing");

    let nothing = run_with(
        App::default(),
        json!({"app": "Mail", "steps": [{"extract": {"what": "results", "into": "rows"}}]}),
        |_| {},
        |_, _, _| None,
    )
    .await;
    assert!(nothing.result.steps[0].note.contains("no list of results"));
}
