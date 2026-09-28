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
    enter, fit, flow_guide, ground, memory, run_flow_with,
    steps::{self, already_chosen, in_region, lists_more_than, looks_like_date, redacted},
    validate, validate_flow,
    view::{Candidate, Depth, Screen},
    vote,
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
    /// A side drawer lies over the page: every click is refused as covered
    /// until Escape closes it.
    Drawer,
    /// Every click is refused: the element is not visible.
    Unclickable,
    /// A list of cities whose rows carry a `combobox` role but take no
    /// text, above the one real search field.
    CityRows,
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
    /// Days in a date strip above the results, a longer list than they are.
    date_strip: usize,
    extra_buttons: usize,
    /// A booking form with an autocomplete destination and a calendar.
    booking: Option<Booking>,
    /// A fare radio shown already checked, as a fare page preselects one.
    checked_fare: Option<&'static str>,
    /// A line of guidance shown on the page, such as a date layout.
    hint: Option<&'static str>,
    quirks: BTreeSet<Quirk>,
}

impl Sim {
    fn has(&self, quirk: Quirk) -> bool {
        self.quirks.contains(&quirk)
    }
}

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// A booking form: a destination box that opens a search field, as an
/// autocomplete does, and a departure date picked only from a calendar.
#[derive(Debug, Default)]
struct Booking {
    /// Whether the destination's search field is open.
    searching: bool,
    /// `Some(month)` while the calendar is open on that month (0 = January).
    calendar: Option<usize>,
}

/// What pressing `name` does to the booking form.
fn press_booking(sim: &mut Sim, name: &str) {
    let Some(booking) = sim.booking.as_mut() else {
        return;
    };
    match name {
        "Going to?" => booking.searching = true,
        "Departure" => booking.calendar = Some(8),
        "Next Month" => booking.calendar = booking.calendar.map(|month| (month + 1) % 12),
        // The calendar's own aggregated-label container also ends with
        // " 2026" (it lists every day of the open month), so this requires
        // the exact "<day> <month> 2026" shape a single day button carries:
        // a bug that let production code ground and press that container
        // instead of a day must not be able to pass this simulated test.
        day if booking.calendar.is_some() && is_single_day_label(day) => {
            booking.calendar = None;
            sim.fields.insert("Departure".to_owned(), day.to_owned());
        }
        _ => {}
    }
}

/// The preselected fare radio, when the simulator shows one.
fn checked_fare(sim: &Sim, root: &str, candidates: &mut Vec<Candidate>) {
    if let Some(fare) = sim.checked_fare {
        let mut radio = node(fare, "radio", &["Click"], &[root, "group \"Fares\""], 200.0);
        radio.states = vec!["checked".to_owned()];
        candidates.push(radio);
    }
}

/// Whether `name` is exactly a single day button's label: `"<day> <month>
/// 2026"`, nothing more. The calendar's own container control names every
/// visible day, so a plain `ends_with(" 2026")` check would also treat
/// pressing that container as picking a day.
fn is_single_day_label(name: &str) -> bool {
    let mut words = name.split(' ');
    let day_is_a_number = words.next().is_some_and(|day| day.parse::<u8>().is_ok());
    day_is_a_number
        && words.next().is_some()
        && words.next() == Some("2026")
        && words.next().is_none()
}

/// The booking form's controls, as they stand.
fn booking_widget(sim: &Sim, booking: &Booking, root: &str, candidates: &mut Vec<Candidate>) {
    let widget = [root, "group \"Booking\""];
    candidates.push(node("Going to?", "button", &["Click"], &widget, 80.0));
    // The destination's own container names its recent searches, so it
    // mentions the option without being it; pressing it chooses nothing.
    candidates.push(node(
        "destinationCity Empty RECENT SEARCHES Srinagar Srinagar International Airport SXR \
         POPULAR DESTINATIONS Mumbai Chhatrapati Shivaji Maharaj International Airport BOM",
        "button",
        &["Click"],
        &widget,
        81.0,
    ));
    if booking.searching {
        // A suggestion row that claims to take text but does not, as
        // IndiGo's comboboxes do.
        candidates.push(node(
            "Mumbai, BOM",
            "combobox",
            &["Click", "SetValue"],
            &widget,
            85.0,
        ));
        let typed = sim.fields.get("Search city").cloned().unwrap_or_default();
        // The box shows what was typed, so it "mentions" the option too.
        let mut search = node(
            "Search city",
            "textbox",
            &["Click", "SetValue"],
            &widget,
            90.0,
        );
        search.value = Some(json!(typed));
        candidates.push(search);
        if !typed.is_empty() && "srinagar".starts_with(&typed.to_lowercase()) {
            candidates.push(node("Srinagar, SXR", "option", &["Click"], &widget, 95.0));
        }
    }
    candidates.push(node("Departure", "button", &["Click"], &widget, 120.0));
    if let Some(month) = booking.calendar {
        // The date field's own label lists the whole open calendar.
        let listing = (1..=28)
            .map(|day| format!("{day} {} 2026", MONTH_NAMES[month]))
            .collect::<Vec<_>>()
            .join(" ");
        candidates.push(node(
            &format!("departureDate Previous Month Next Month {listing}"),
            "button",
            &["Click"],
            &widget,
            125.0,
        ));
        candidates.push(node("Next Month", "button", &["Click"], &widget, 130.0));
        for day in 1..=28 {
            let name = format!("{day} {} 2026", MONTH_NAMES[month]);
            candidates.push(node(&name, "button", &["Click"], &widget, 140.0));
        }
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

/// A city list whose unnamed rows each hold their city as a value and take
/// no text, above the one real search field.
fn city_rows(root: &str, candidates: &mut Vec<Candidate>) {
    for (index, city) in ["Mumbai", "Pune", "Chennai"].into_iter().enumerate() {
        candidates.push(Candidate {
            ref_id: format!("@s:city-{index}"),
            role: "combobox".to_owned(),
            value: Some(json!(city)),
            available_actions: vec!["Click".to_owned(), "SetValue".to_owned()],
            path: vec![root.to_owned(), "list \"Cities\"".to_owned()],
            ..Candidate::default()
        });
    }
    candidates.push(node(
        "Search",
        "textfield",
        &["SetValue"],
        &[root, "group \"Where to\""],
        210.0,
    ));
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
    for day in 0..sim.date_strip {
        let path = vec![
            root.to_owned(),
            "list \"Dates\"".to_owned(),
            format!("listitem #{}", day + 1),
        ];
        text_nodes.push(Candidate {
            role: "text".to_owned(),
            value: Some(json!("--")),
            path: path.clone(),
            order: 500 + day * 10,
            ..Candidate::default()
        });
        candidates.push(Candidate {
            ref_id: format!("@s:day-{}", day + 1),
            role: "button".to_owned(),
            name: Some(format!("Please Select Date for {} Oct", day + 12)),
            available_actions: vec!["Click".to_owned()],
            path,
            order: 500 + day * 10 + 5,
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
            checked_fare(&sim, &root, &mut candidates);
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
        if let Some(booking) = &sim.booking {
            booking_widget(&sim, booking, &root, &mut candidates);
        }
        if sim.has(Quirk::CityRows) {
            city_rows(&root, &mut candidates);
        }
        let text_nodes = result_cards(&sim, &root, &mut candidates);
        let mut surface = "window".to_owned();
        if sim.obstacle {
            surface = "sheet".to_owned();
            obstacle_sheet(&mut candidates);
        }
        Screen {
            app: "Mail".to_owned(),
            window: Some(window.to_owned()),
            surface,
            candidates,
            context: std::iter::once(format!("{window} heading"))
                .chain(sim.hint.map(str::to_owned))
                .collect(),
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
                tinycomputer_bus::DesktopError::new("APP_NOT_FOUND", "no such app"),
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
        if sim.has(Quirk::Unclickable) && operation == JevOperation::Click {
            return DesktopResponse::err(
                "click",
                tinycomputer_bus::DesktopError::new(
                    "NOT_ACTIONABLE",
                    "Element exists but is not visible.",
                ),
            );
        }
        if sim.has(Quirk::Drawer) && operation == JevOperation::Click {
            return DesktopResponse::err(
                "click",
                tinycomputer_bus::DesktopError::new(
                    "NOT_ACTIONABLE",
                    format!("Element '@s:{name}' is covered by <div.drawer> at its click point"),
                ),
            );
        }
        if is_city_row(target.as_ref()) && operation == JevOperation::TypeText {
            return not_a_text_field();
        }
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
                    _ if sim.booking.is_some() => press_booking(&mut sim, &name),
                    _ => {}
                }
            }
            JevOperation::TypeText if name == "Mumbai, BOM" => {}
            // Text with no target goes to the focused field: the booking
            // form's search box once it is open.
            JevOperation::TypeText if target.is_none() => {
                sim.fields
                    .insert("Search city".to_owned(), text.unwrap_or_default());
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
        if is_city_row(Some(target)) {
            return not_a_text_field();
        }
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
            "escape" => {
                sim.obstacle = false;
                sim.quirks.remove(&Quirk::Drawer);
            }
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
                tinycomputer_bus::DesktopError::new("APP_NOT_FOUND", "no such app"),
            );
        }
        DesktopResponse::ok("launch", json!({}))
    }

    fn navigate(&self, url: &str) -> DesktopResponse {
        let mut sim = self.sim();
        if sim.has(Quirk::NoAddresses) {
            return DesktopResponse::err(
                "navigate",
                tinycomputer_bus::DesktopError::new("ACTION_NOT_SUPPORTED", "no addresses"),
            );
        }
        sim.navigated.push(url.to_owned());
        DesktopResponse::ok("navigate", json!({"url": url, "title": "Flights"}))
    }
}

fn is_city_row(target: Option<&Candidate>) -> bool {
    target.is_some_and(|target| target.ref_id.starts_with("@s:city-"))
}

fn not_a_text_field() -> DesktopResponse {
    DesktopResponse::err(
        "type-text",
        tinycomputer_bus::DesktopError::new("NOT_A_TEXT_FIELD", "no input takes the text"),
    )
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
        // Every action helps and no field shows an error, unless a test says.
        "confirm" | "helped" | "dismiss_known" => noul(0.9),
        _ if id.starts_with("known_") => noul(0.9),
        // A survey finds the step in "Region 1" and nothing distracting.
        _ if id.starts_with("relevance_") => {
            level(if text_of(question, "region").contains("region 1") {
                4
            } else {
                1
            })
        }
        _ if id.starts_with("distraction_") => noul(0.05),
        _ if id.starts_with("error_") => noul(0.05),
        _ if id.starts_with("asks_") => noul(0.9),
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
async fn a_fact_is_typed_through_enter_but_never_reaches_a_jev_request() {
    let app = App::quirky(Quirk::BodyIgnoresSetValue);
    let run = run_with(
        app,
        mail_flow(),
        |request| {
            request.facts = BTreeSet::from(["to".to_owned()]);
            // A task's flow always runs with `include_values` off (screen
            // text is a separate, already-guarded leak path); this test is
            // about `${to}` substitution, the bug this change fixes.
            request.include_values = false;
        },
        |_, _, _| None,
    )
    .await;

    assert_eq!(run.result.stop, FlowStopReason::StoppedBeforeDestructive);
    let sim = run.app.sim();
    assert_eq!(
        sim.fields["To"], "sam@example.com",
        "the fact is still typed into the field"
    );

    let leaked = run.requests.iter().any(|request| {
        serde_json::to_string(request)
            .unwrap()
            .contains("sam@example.com")
    });
    assert!(!leaked, "a fact's value must never reach a Jev request");
    assert!(
        run.result
            .steps
            .iter()
            .all(|step| !step.text.contains("sam@example.com")
                && !step.note.contains("sam@example.com")),
        "a fact's value must not appear in a step report either"
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
    assert_eq!(
        asked(&run.requests, "region"),
        1,
        "one region question, asked alongside the knockout, not a round per level"
    );
    assert_eq!(asked(&run.requests, "group_0"), 1);
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
async fn a_covered_click_closes_what_covers_it_and_tries_again() {
    let run = run_with(
        App::quirky(Quirk::Drawer),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |_| {},
        |id, question, _| (id == "move").then(|| pick(question, "activate", 0.9)),
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    let sim = run.app.sim();
    assert!(sim.compose_open);
    assert_eq!(sim.presses, ["escape"]);
    let actions = &run.result.steps[0].actions;
    assert_eq!(
        actions
            .iter()
            .map(|action| action.action.as_str())
            .collect::<Vec<_>>(),
        ["click", "press escape (uncover)", "click"],
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
    // With no completion judge to overrule it, "finished" ends the step.
    let finished = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy up"]}),
        |request| request.disabled_loops = vec![FlowLoop::Completion],
        |id, question, _| (id == "move").then(|| pick(question, "finished", 0.9)),
    )
    .await;
    assert_eq!(finished.result.stop, FlowStopReason::Completed);

    // A judge that sees the step undone overrules it: the loop acts instead
    // of skipping a step that was never done.
    let overruled = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy up"]}),
        |request| request.max_actions = 3,
        |id, question, _| (id == "move").then(|| pick(question, "finished", 0.9)),
    )
    .await;
    assert_ne!(overruled.result.stop, FlowStopReason::Completed);
    assert!(!overruled.app.sim().clicks.is_empty(), "it acted instead");
    assert!(overruled.requests.iter().any(|request| {
        serde_json::to_string(&request.state)
            .unwrap()
            .contains("does not yet clearly show this step done")
    }));

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
async fn choose_types_into_an_autocomplete_and_picks_the_suggestion() {
    // A planner may qualify the option; the box is searched by its name.
    for option in ["Srinagar", "Srinagar (SXR)"] {
        let run = run_with(
            App::with(|sim| sim.booking = Some(Booking::default())),
            json!({"app": "Mail", "steps": [
                {"choose": {"what": "the destination box", "option": option}}
            ]}),
            |_| {},
            |id, question, sim| match id {
                "move" => Some(pick(question, "activate", 0.9)),
                "done" => Some(noul(
                    if sim
                        .booking
                        .as_ref()
                        .is_some_and(|booking| booking.searching)
                    {
                        0.9
                    } else {
                        0.05
                    },
                )),
                _ if !matches!(question, Question::Choice(_)) => None,
                _ if purpose_of(question).contains("search box") => {
                    Some(pick(question, "Mumbai", 0.9))
                }
                _ if purpose_of(question).contains("open the destination") => {
                    Some(pick(question, "Going to?", 0.9))
                }
                _ => Some(pick(question, "Srinagar", 0.9)),
            },
        )
        .await;
        assert_eq!(
            run.result.stop,
            FlowStopReason::Completed,
            "{:?}",
            run.result.steps
        );
        let sim = run.app.sim();
        assert_eq!(
            sim.fields["Search city"], "Srinagar",
            "a row that takes no text leaves the typing to the focused box"
        );
        assert!(!sim.clicks.contains(&"Mumbai, BOM".to_owned()));
        assert_eq!(
            sim.clicks.last().map(String::as_str),
            Some("Srinagar, SXR"),
            "the box itself is never taken for the option: {:?}",
            sim.clicks
        );
    }
}

fn purpose_of(question: &Question) -> String {
    text_of(question, "purpose")
}

#[tokio::test]
async fn enter_picks_a_date_from_a_calendar_without_telling_jev_the_date() {
    let run = run_with(
        App::with(|sim| sim.booking = Some(Booking::default())),
        json!({"app": "Mail", "steps": [{"enter": {"departure date": "Sunday, 18 October 2026"}}]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "done" => Some(noul(
                if sim
                    .booking
                    .as_ref()
                    .is_some_and(|booking| booking.calendar.is_some())
                {
                    0.9
                } else {
                    0.05
                },
            )),
            _ if !matches!(question, Question::Choice(_)) => None,
            _ if id.starts_with("slot_") => Some(pick(question, "none", 0.9)),
            _ if purpose_of(question).contains("value being entered") => {
                Some(pick(question, "18 October", 0.9))
            }
            _ => Some(pick(question, "Departure", 0.9)),
        },
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    let sim = run.app.sim();
    assert_eq!(sim.fields["Departure"], "18 October 2026");
    assert_eq!(
        sim.clicks
            .iter()
            .filter(|click| *click == "Next Month")
            .count(),
        1,
        "paged from September to October: {:?}",
        sim.clicks
    );
    for request in &run.requests {
        for question in request.questions.values() {
            for field in ["purpose", "step", "task"] {
                assert!(
                    !text_of(question, field).contains("18 october"),
                    "the value reached a question: {}",
                    text_of(question, field)
                );
            }
        }
    }
}

#[test]
fn a_date_is_told_from_other_options_and_containers_give_way() {
    use super::steps::{closest, looks_like_date};
    assert!(looks_like_date("Sunday 18 October 2026"));
    assert!(looks_like_date("october 3"));
    assert!(!looks_like_date("18 oct"), "a month must be spelled out");
    assert!(!looks_like_date("October"), "a month alone is no day");
    assert!(!looks_like_date("Srinagar 40"));

    let day = node("Sunday, 18 October 2026", "button", &["Click"], &[], 0.0);
    let month = node(
        &format!("departureDate {}", "Sunday, 18 October 2026 ".repeat(20)),
        "button",
        &["Click"],
        &[],
        0.0,
    );
    let names = |kept: Vec<Candidate>| {
        kept.into_iter()
            .filter_map(|node| node.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(closest(vec![month.clone(), day.clone()])),
        [day.name.clone().unwrap()]
    );
    assert_eq!(
        names(closest(vec![month.clone()])).len(),
        1,
        "a lone match stays"
    );
    assert!(closest(Vec::new()).is_empty());
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
        // Every way to find a field is tried before the step fails.
        |request| request.max_actions = 20,
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
        // Every way to find a field is tried before the step fails.
        |request| request.max_actions = 20,
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

    let fact_in_condition = run_flow_with(
        App::default(),
        &runtime(Oracle {
            app: App::default(),
            hook: Box::new(|_, _, _| None),
            requests: Mutex::new(Vec::new()),
            fail: false,
        }),
        RunFlowRequest {
            flow: serde_json::from_value(json!({"app": "Mail", "steps": [
                {"verify": "shows ${email}"}
            ]}))
            .unwrap(),
            vars: BTreeMap::from([("email".to_owned(), "sam@example.com".to_owned())]),
            facts: BTreeSet::from(["email".to_owned()]),
            ..RunFlowRequest::default()
        },
    )
    .await;
    let error = fact_in_condition.error.unwrap();
    assert_eq!(error.code, "FLOW_INVALID");
    assert!(
        error.message.contains("is a secret"),
        "a fact referenced outside an enter step never starts running: {error:?}"
    );
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
    let validation: tinycomputer_bus::FlowValidation =
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
        &BTreeSet::new(),
    );
    assert!(malformed.0.is_none());
    assert!(malformed.1.errors[0].contains("not well formed"));

    let empty = validate::check(&Flow::default(), &BTreeSet::new(), &BTreeSet::new());
    assert!(
        empty
            .errors
            .iter()
            .any(|error| error.contains("at least one step"))
    );

    let huge = Flow {
        app: "Mail".to_owned(),
        vars: BTreeMap::new(),
        steps: vec![tinycomputer_bus::FlowStep::Intent("x".to_owned()); 101],
    };
    assert!(
        validate::check(&huge, &BTreeSet::new(), &BTreeSet::new()).errors[0]
            .contains("at most 100")
    );

    let ok = validate::validate(&mail_flow(), &BTreeSet::new(), &BTreeSet::new());
    assert!(ok.0.is_some() && ok.1.valid && ok.1.steps == 5);
    let with_runtime_var = validate::check(
        &serde_json::from_value(json!({"app": "Mail", "steps": [{"open": "${app}"}]})).unwrap(),
        &BTreeSet::from(["app".to_owned()]),
        &BTreeSet::new(),
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

#[tokio::test]
async fn a_flow_definition_naming_a_caller_value_is_expanded_once() {
    let run = run_with(
        App::with(|sim| sim.compose_open = true),
        json!({
            "app": "Mail",
            "vars": {"subject_line": "${topic}"},
            "steps": [{"enter": {"subject": "${subject_line}", "body": "${note}"}}]
        }),
        |request| {
            request.vars = BTreeMap::from([
                ("topic".to_owned(), "Kashmir".to_owned()),
                ("note".to_owned(), "${topic}".to_owned()),
            ]);
        },
        |_, _, _| None,
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    let sim = run.app.sim();
    assert_eq!(
        sim.fields["Subject"], "Kashmir",
        "the definition names the caller's value"
    );
    assert_eq!(
        sim.fields["Body"], "${topic}",
        "a caller's value is text, never rescanned for references"
    );
}

#[tokio::test]
async fn a_flow_definition_naming_a_fact_is_typed_but_never_reaches_a_jev_request() {
    let run = run_with(
        App::with(|sim| sim.compose_open = true),
        json!({
            "app": "Mail",
            "vars": {"subject_line": "${topic}"},
            "steps": [{"enter": {"subject": "${subject_line}"}}]
        }),
        |request| {
            request.vars = BTreeMap::from([("topic".to_owned(), "Kashmir".to_owned())]);
            request.facts = BTreeSet::from(["topic".to_owned()]);
            request.include_values = false;
        },
        |_, _, _| None,
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    assert_eq!(
        run.app.sim().fields["Subject"],
        "Kashmir",
        "the definition carries the fact into the field"
    );
    let leaked = run
        .requests
        .iter()
        .any(|request| serde_json::to_string(request).unwrap().contains("Kashmir"));
    assert!(!leaked, "a fact's value must never reach a Jev request");
}

#[test]
fn validation_treats_a_flow_definition_naming_a_fact_as_a_fact() {
    let facts = BTreeSet::from(["email".to_owned()]);
    let flow: Flow = serde_json::from_value(json!({
        "app": "Mail",
        "vars": {"recipient": "${email}", "topic": "the budget"},
        "steps": [
            {"do": "write to ${recipient} about ${topic}"},
            {"enter": {"to": "${recipient}"}}
        ]
    }))
    .unwrap();
    let validation = validate::check(&flow, &facts, &facts);
    assert_eq!(
        validation.errors,
        vec![
            "step 1: `${recipient}` is a secret; only an enter step may type it — Jev only ever sees it as a name"
                .to_owned()
        ],
        "only the model-facing use of the fact-bearing definition is rejected"
    );
    assert_eq!(
        validate::carrying_facts(&flow.vars, &facts),
        BTreeSet::from(["email".to_owned(), "recipient".to_owned()]),
        "a definition naming a fact carries it; one that does not stays ordinary"
    );
}

#[test]
fn validation_rejects_a_fact_referenced_in_every_model_facing_position() {
    let flow: Flow = serde_json::from_value(json!({
        "app": "Mail",
        "steps": [
            {"open": "${email}"},
            {"browse": "${email}"},
            {"do": "tell Jev ${email}"},
            {"verify": "shows ${email}"},
            {"wait_for": "shows ${email}"},
            {"stop_before": "sending to ${email}"},
            {"choose": {"what": "${email}", "option": "ok"}},
            {"choose": {"what": "list", "option": "${email}"}},
            {"read": {"what": "the ${email} row", "into": "x"}},
            {"extract": {"what": "the ${email} row", "into": "y"}},
            {"pick": {"from": "${email}", "by": "lowest", "into": "z"}},
            {"pick": {"from": "results", "by": "${email}", "into": "w"}},
            {"repeat_until": {"condition": "shows ${email}", "steps": ["x"], "max": 1}},
            {"if": {"condition": "shows ${email}", "then": ["x"]}},
            {"enter": {"${email}": "hi"}}
        ]
    }))
    .unwrap();
    let facts = BTreeSet::from(["email".to_owned()]);
    let validation = validate::check(&flow, &facts, &facts);
    assert!(!validation.valid);
    let hits = validation
        .errors
        .iter()
        .filter(|error| error.contains("`${email}` is a secret"))
        .count();
    assert_eq!(
        hits,
        15,
        "every model-facing position should reject the fact:\n{}",
        validation.errors.join("\n")
    );
    for error in &validation.errors {
        if error.contains("`${email}` is a secret") {
            assert!(error.contains("only an enter step may type it"), "{error}");
        }
    }
}

#[test]
fn validation_allows_a_fact_only_as_an_enter_steps_typed_value() {
    let flow: Flow = serde_json::from_value(json!({
        "app": "Mail",
        "steps": [
            {"enter": {"email address": "${email}"}}
        ]
    }))
    .unwrap();
    let facts = BTreeSet::from(["email".to_owned()]);
    let validation = validate::check(&flow, &facts, &facts);
    assert!(validation.valid, "{:?}", validation.errors);
}

#[test]
fn validation_rejects_a_fact_reached_by_open_or_browse() {
    // The launched application or address becomes `screen.app` and a step
    // note in `history`, both of which reach Jev on a later step, so these
    // are rejected the same as any other model-facing position.
    let open = validate::check(
        &serde_json::from_value(json!({"app": "Mail", "steps": [{"open": "${app_name}"}]}))
            .unwrap(),
        &BTreeSet::from(["app_name".to_owned()]),
        &BTreeSet::from(["app_name".to_owned()]),
    );
    assert!(
        open.errors
            .iter()
            .any(|error| error.contains("`${app_name}` is a secret"))
    );

    let browse = validate::check(
        &serde_json::from_value(json!({"app": "browser", "steps": [{"browse": "${site}"}]}))
            .unwrap(),
        &BTreeSet::from(["site".to_owned()]),
        &BTreeSet::from(["site".to_owned()]),
    );
    assert!(
        browse
            .errors
            .iter()
            .any(|error| error.contains("`${site}` is a secret"))
    );
}

#[test]
fn validation_allows_a_non_fact_variable_in_model_facing_text() {
    let flow: Flow = serde_json::from_value(json!({
        "app": "Mail",
        "vars": {"topic": "the budget"},
        "steps": [
            {"do": "start an email about ${topic}"},
            {"verify": "mentions ${topic}"}
        ]
    }))
    .unwrap();
    let validation = validate::check(&flow, &BTreeSet::new(), &BTreeSet::new());
    assert!(validation.valid, "{:?}", validation.errors);
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
fn substitute_safe_never_expands_a_fact_even_if_asked_to() {
    let vars = BTreeMap::from([
        ("email".to_owned(), "sam@example.com".to_owned()),
        ("topic".to_owned(), "budget".to_owned()),
    ]);
    let facts = BTreeSet::from(["email".to_owned()]);
    assert_eq!(
        validate::substitute_safe("send to ${email} about ${topic}", &vars, &facts),
        "send to ${email} about budget"
    );
    assert_eq!(
        validate::substitute_safe("${email}", &vars, &BTreeSet::new()),
        "sam@example.com",
        "a non-fact name still substitutes normally"
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
async fn pick_ranks_the_list_that_has_prices_not_the_longest_one() {
    let app = flights();
    app.sim().date_strip = 7;
    let run = run(
        app,
        json!({"app": "Mail", "steps": [
            {"pick": {"from": "the flight results", "by": "lowest price", "into": "flight"}}
        ]}),
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    assert_eq!(run.app.sim().picked, ["@s:select-1"]);
    assert!(run.result.vars["flight"].starts_with("IndiGo"));
    assert!(run.result.steps[0].note.contains("ranked"));
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
        super::validate::check(
            &serde_json::from_value(flow).unwrap(),
            &BTreeSet::new(),
            &BTreeSet::new(),
        )
        .errors
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

// ------------------------------------------------ brief, secrets, and votes

/// The brief a request's choosing questions carry; `Null` when none does.
fn brief_of(request: &EvaluationRequest) -> Value {
    request
        .questions
        .values()
        .find_map(|question| match question {
            Question::Choice(choice) => choice.instructions.get("brief").cloned(),
            _ => None,
        })
        .unwrap_or(Value::Null)
}

/// Whether any yes/no or scale question about the screen carries a brief.
fn judgements_are_briefed(request: &EvaluationRequest) -> bool {
    request
        .questions
        .iter()
        .any(|(id, question)| match question {
            Question::Noul(noul) => id != "confirm" && noul.instructions.get("brief").is_some(),
            Question::Score(score) => score.instructions.get("brief").is_some(),
            Question::Choice(_) => false,
        })
}

#[tokio::test]
async fn every_question_is_briefed_on_the_goal_the_person_and_the_plan() {
    let run = run_with(
        App::default(),
        mail_flow(),
        |request| {
            request.brief = tinycomputer_bus::FlowBrief {
                goal: "move Thursday's sync with Sam to Friday".to_owned(),
                details: BTreeMap::from([
                    ("first name".to_owned(), "Alex".to_owned()),
                    ("date of birth".to_owned(), "2000-01-01".to_owned()),
                ]),
                secrets: vec!["card number".to_owned()],
                rules: vec!["never send without approval".to_owned()],
            };
        },
        |_, _, _| None,
    )
    .await;
    assert!(!run.requests.is_empty());
    assert!(
        !run.requests.iter().any(judgements_are_briefed),
        "judging the screen is left to the screen"
    );
    assert!(
        run.requests
            .iter()
            .all(|request| request.state.get("brief").is_none())
    );
    let choosing = run
        .requests
        .iter()
        .filter(|request| {
            request
                .questions
                .values()
                .any(|question| matches!(question, Question::Choice(_)))
        })
        .collect::<Vec<_>>();
    assert!(!choosing.is_empty());
    for request in choosing {
        let brief = brief_of(request);
        assert_eq!(brief["goal"], "move Thursday's sync with Sam to Friday");
        assert_eq!(brief["for"]["date of birth"], "2000-01-01");
        assert_eq!(brief["secrets"]["names"], json!(["${card number}"]));
        assert_eq!(brief["rules"], json!(["never send without approval"]));
        assert_eq!(brief["plan"].as_array().unwrap().len(), 5);
    }
    let entering = run
        .requests
        .iter()
        .find(|request| request.questions.keys().any(|id| id.starts_with("slot_")))
        .unwrap();
    let plan = brief_of(entering)["plan"].clone();
    assert_eq!(plan[1], "2. [done] do: start a new email message");
    assert!(
        plan[2].as_str().unwrap().starts_with("3. [now] enter:"),
        "{plan}"
    );
    assert!(
        plan[3].as_str().unwrap().starts_with("4. [next] verify:"),
        "{plan}"
    );
    let sending = run
        .requests
        .iter()
        .rev()
        .find(|request| request.questions.contains_key("target"))
        .unwrap();
    assert_eq!(
        brief_of(sending)["so_far"],
        json!(["entered: message body, recipient, subject"])
    );
}

#[tokio::test]
async fn an_unbriefed_run_sends_no_brief_but_its_plan() {
    let run = run(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
    )
    .await;
    assert!(
        run.requests
            .iter()
            .all(|request| brief_of(request).is_null()),
        "a one-step flow with no brief adds nothing to the state"
    );
}

#[tokio::test]
async fn a_secret_the_page_shows_back_is_masked_in_every_request() {
    let run = run_with(
        App::default(),
        json!({
            "app": "Mail",
            "steps": [
                {"open": "Mail"},
                "start a new email message",
                {"enter": {"message body": "${card number}"}},
                {"verify": "the draft shows the body"}
            ]
        }),
        |request| {
            request.vars =
                BTreeMap::from([("card number".to_owned(), "4111111111111111".to_owned())]);
            request.facts = BTreeSet::from(["card number".to_owned()]);
            // The body field shows what was typed, so the value is on screen.
            request.include_values = true;
        },
        |_, _, _| None,
    )
    .await;
    assert_eq!(run.app.sim().fields["Body"], "4111111111111111");
    let text = run
        .requests
        .iter()
        .map(|request| serde_json::to_string(request).unwrap())
        .collect::<String>();
    assert!(!text.contains("4111111111111111"), "the secret leaked");
    assert!(
        text.contains("${card number}"),
        "the page's copy reads as its template"
    );
    let traced = serde_json::to_string(&run.result.trace).unwrap();
    assert!(!traced.contains("4111111111111111"));
}

#[tokio::test]
async fn a_shared_value_may_be_named_in_a_step_and_reaches_jev() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": [
            {"open": "Mail"},
            "start a new email message",
            {"verify": "the draft is addressed to ${to}"}
        ]}),
        |request| {
            request.vars = BTreeMap::from([("to".to_owned(), "sam@example.com".to_owned())]);
        },
        |_, _, _| None,
    )
    .await;
    assert!(run.requests.iter().any(|request| {
        text_of(
            request
                .questions
                .get("holds")
                .unwrap_or(&ask::condition("")),
            "condition",
        )
        .contains("sam@example.com")
    }));
}

/// A Jev that leans toward whichever option is listed first: the needle gets
/// 0.4, the first option 0.6 — enough to mislead any single asking.
fn first_biased(question: &Question, needle: &str) -> Answer {
    let Question::Choice(choice) = question else {
        panic!("a choice");
    };
    let right = choice
        .criteria
        .iter()
        .find(|(_, description)| {
            description
                .as_ref()
                .is_some_and(|description| description.to_string().contains(needle))
        })
        .map(|(key, _)| key.clone())
        .unwrap();
    let first = choice
        .criteria
        .keys()
        .find(|key| *key != "none")
        .unwrap()
        .clone();
    let probabilities = choice
        .criteria
        .keys()
        .map(|key| {
            let mut probability = 0.0;
            if *key == right {
                probability += 0.4;
            }
            if *key == first {
                probability += 0.6;
            }
            (key.clone(), probability)
        })
        .collect::<BTreeMap<String, f64>>();
    let choice = probabilities
        .iter()
        .max_by(|left, right| left.1.total_cmp(right.1))
        .unwrap()
        .0
        .clone();
    Answer::Choice(ChoiceAnswer {
        choice,
        probabilities,
        confidence: 0.6,
    })
}

async fn biased_mail(votes: u32) -> Run {
    run_with(
        App::default(),
        json!({"app": "Mail", "steps": [
            {"open": "Mail"},
            "start a new email message",
            {"enter": {"subject": "Moving Thursday's sync"}}
        ]}),
        move |request| request.votes = votes,
        |id, question, _| {
            id.starts_with("slot_")
                .then(|| first_biased(question, needle_for(&text_of(question, "purpose"))))
        },
    )
    .await
}

#[tokio::test]
async fn a_vote_undoes_a_bias_that_misleads_a_single_asking() {
    let once = biased_mail(1).await;
    assert_ne!(
        once.app.sim().fields.get("Subject").map(String::as_str),
        Some("Moving Thursday's sync"),
        "asked once, the first field wins"
    );
    let voted = biased_mail(5).await;
    assert_eq!(
        voted.app.sim().fields["Subject"],
        "Moving Thursday's sync",
        "asked five ways, the right field wins"
    );
    assert!(voted.result.steps[2].loops.contains(&FlowLoop::Vote));
}

#[tokio::test]
async fn every_framing_is_charged_and_the_budget_bounds_them() {
    let flow = json!({"app": "Mail", "steps": [{"open": "Mail"}, "start a new email message"]});
    let once = run_with(App::default(), flow.clone(), |_| {}, |_, _, _| None).await;
    let thrice = run_with(
        App::default(),
        flow.clone(),
        |request| request.votes = 3,
        |_, _, _| None,
    )
    .await;
    assert_eq!(thrice.result.metrics.calls, 3 * once.result.metrics.calls);
    assert_eq!(
        usize::try_from(thrice.result.metrics.calls).unwrap(),
        thrice.requests.len()
    );
    assert_eq!(
        thrice.result.trace.len(),
        once.result.trace.len(),
        "the trace keeps one merged exchange per decision"
    );
    let squeezed = run_with(
        App::default(),
        flow,
        |request| {
            request.votes = 9;
            request.max_model_calls = 2;
        },
        |_, _, _| None,
    )
    .await;
    assert_eq!(squeezed.result.metrics.calls, 2, "voting never overspends");
    assert_eq!(squeezed.result.stop, FlowStopReason::ModelBudget);
}

#[test]
fn framings_relabel_label_keys_and_keep_word_keys() {
    let request = ask::request(
        "jev-latest",
        json!({}),
        ask::Questions::default()
            .with(
                "target",
                ask::options(
                    json!({"task": "t"}),
                    ["1", "2", "3"].map(|key| (key.to_owned(), json!(format!("option {key}")))),
                ),
            )
            .with(
                "move",
                ask::options(
                    json!({"task": "m"}),
                    ["activate", "wait"].map(|key| (key.to_owned(), json!(key))),
                ),
            )
            .with("done", ask::completion("x")),
    );
    let framings = vote::framings(&request, 4);
    assert_eq!(framings.len(), 4);
    assert_eq!(
        framings[0].request, request,
        "the first framing is the request"
    );
    let firsts = framings
        .iter()
        .map(|framing| {
            let Question::Choice(choice) = &framing.request.questions["target"] else {
                panic!()
            };
            let first = choice.criteria.keys().next().unwrap();
            choice.criteria[first].clone().unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        firsts,
        [
            json!("option 1"),
            json!("option 2"),
            json!("option 3"),
            json!("option 1")
        ],
        "each framing leads with another option"
    );
    let Question::Choice(target) = &framings[1].request.questions["target"] else {
        panic!()
    };
    assert_eq!(
        target.criteria.keys().collect::<Vec<_>>(),
        ["A", "B", "C", "none"]
    );
    assert!(
        !target.instructions["perspective"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    let Question::Choice(moves) = &framings[1].request.questions["move"] else {
        panic!()
    };
    assert_eq!(
        moves.criteria.keys().collect::<Vec<_>>(),
        ["activate", "none", "wait"],
        "meaningful keys are kept"
    );
    assert_eq!(vote::framings(&request, 50).len(), vote::MAX_VOTES as usize);
    assert_eq!(vote::framings(&request, 0).len(), 1);
}

#[test]
fn merged_answers_average_under_the_original_keys() {
    let request = ask::request(
        "jev-latest",
        json!({}),
        ask::Questions::default()
            .with(
                "target",
                ask::options(
                    json!({"task": "t"}),
                    (1..=30).map(|key| (key.to_string(), json!(format!("option {key}")))),
                ),
            )
            .with("done", ask::completion("x"))
            .with("progress", ask::progress("x")),
    );
    let framings = vote::framings(&request, 2);
    let Question::Choice(second) = &framings[1].request.questions["target"] else {
        panic!()
    };
    let keys = second.criteria.keys().cloned().collect::<Vec<_>>();
    assert_eq!((keys[0].as_str(), keys[29].as_str()), ("AA", "BD"));
    let key_for = |framing: &vote::Framing, description: &str| {
        let Question::Choice(choice) = &framing.request.questions["target"] else {
            panic!()
        };
        choice
            .criteria
            .iter()
            .find(|(_, value)| value.as_ref() == Some(&json!(description)))
            .unwrap()
            .0
            .clone()
    };
    let answer = |framing: &vote::Framing, winner: &str, done: f64, top: usize| {
        let winner = key_for(framing, winner);
        BTreeMap::from([
            (
                "target".to_owned(),
                Answer::Choice(ChoiceAnswer {
                    probabilities: BTreeMap::from([(winner.clone(), 0.8)]),
                    choice: winner,
                    confidence: 0.8,
                }),
            ),
            ("done".to_owned(), noul(done)),
            ("progress".to_owned(), level(top)),
        ])
    };
    let answered = vec![
        (
            framings[0].clone(),
            answer(&framings[0], "option 7", 0.9, 4),
        ),
        (
            framings[1].clone(),
            answer(&framings[1], "option 7", 0.5, 2),
        ),
    ];
    let merged = vote::merge(&answered);
    let Answer::Choice(target) = &merged["target"] else {
        panic!()
    };
    assert_eq!(target.choice, "7");
    assert!((target.probabilities["7"] - 0.8).abs() < 1e-9);
    assert!((target.confidence - 1.0).abs() < 1e-9);
    assert!((ask::probability(&merged, "done").unwrap() - 0.7).abs() < 1e-9);
    assert!((ask::top_level(&merged, "progress").unwrap() - 0.5).abs() < 1e-9);

    let split = vote::merge(&[
        (
            framings[0].clone(),
            answer(&framings[0], "option 7", 0.9, 4),
        ),
        (
            framings[1].clone(),
            answer(&framings[1], "option 9", 0.9, 4),
        ),
    ]);
    let Answer::Choice(split) = &split["target"] else {
        panic!()
    };
    assert!((split.confidence - 0.5).abs() < 1e-9, "one of two agreed");
    assert!(vote::merge(&[]).is_empty());
}

#[tokio::test]
async fn a_web_page_is_named_and_the_name_briefs_the_next_question() {
    let web = run_with(
        App::default(),
        json!({"app": "browser", "steps": [
            {"browse": "https://flights.test"},
            {"verify": "flights are listed"},
            {"stop_before": "booking the flight"}
        ]}),
        |_| {},
        |id, question, _| match id {
            "page_kind" => Some(pick(question, "results", 0.9)),
            "holds" => Some(noul(0.9)),
            _ => None,
        },
    )
    .await;
    let named = web
        .requests
        .iter()
        .filter(|request| request.questions.contains_key("page_kind"))
        .count();
    assert!(named > 0, "a web page's questions carry the page kind");
    assert!(
        web.requests
            .iter()
            .any(|request| brief_of(request)["page"] == "results"),
        "a later question is told the page is a results page"
    );
    assert!(web.result.steps[1].loops.contains(&FlowLoop::PageKind));
    let desktop = run(App::default(), mail_flow()).await;
    assert!(
        desktop
            .requests
            .iter()
            .all(|request| !request.questions.contains_key("page_kind")),
        "a desktop application has no page kind"
    );
}

#[tokio::test]
async fn an_action_judged_unhelpful_is_undone_and_not_tried_again() {
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
            "helped" => Some(noul(if sim.clicks.is_empty() { 0.9 } else { 0.05 })),
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
async fn a_field_the_form_flags_is_entered_again_and_then_gives_up() {
    let flow = json!({"app": "Mail", "steps": [
        {"open": "Mail"},
        "start a new email message",
        {"enter": {"subject": "Moving Thursday's sync"}}
    ]});
    let asked = Arc::new(Mutex::new(0));
    let counter = asked.clone();
    let recovered = run_with(
        App::default(),
        flow.clone(),
        |_| {},
        move |id, _, _| {
            (id == "error_0").then(|| {
                let mut asked = counter.lock().unwrap();
                *asked += 1;
                noul(if *asked == 1 { 0.9 } else { 0.05 })
            })
        },
    )
    .await;
    assert_eq!(recovered.result.stop, FlowStopReason::Completed);
    assert!(
        recovered.result.steps[2]
            .loops
            .contains(&FlowLoop::Validation)
    );
    let fills = recovered.result.steps[2]
        .actions
        .iter()
        .filter(|action| action.action.starts_with("fill"))
        .count();
    assert_eq!(fills, 2, "the flagged field is entered twice");

    let stuck = run_with(
        App::default(),
        flow,
        |_| {},
        |id, _, _| (id == "error_0").then(|| noul(0.9)),
    )
    .await;
    assert_eq!(stuck.result.stop, FlowStopReason::StepFailed);
    assert!(
        stuck.result.steps[2]
            .note
            .contains("still shows an error about: subject"),
        "{}",
        stuck.result.steps[2].note
    );
}

#[tokio::test]
async fn waits_that_change_nothing_are_not_a_stall_and_stop_being_offered() {
    let run = run_with(
        App::quirky(Quirk::Frozen),
        json!({"app": "Mail", "steps": ["open the search results"]}),
        |_| {},
        |id, question, _| (id == "move").then(|| pick(question, "wait", 0.9)),
    )
    .await;
    let step = &run.result.steps[0];
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(
        step.note.contains("not accomplished after"),
        "a settled page is not a stall: {}",
        step.note
    );
    let waits = step
        .actions
        .iter()
        .filter(|action| action.action == "wait")
        .count();
    assert_eq!(waits, 2, "no third wait on a settled page");
    let told = run.requests.iter().any(|request| {
        serde_json::to_string(&request.state)
            .unwrap()
            .contains("the page has finished loading and nothing changed")
    });
    assert!(told, "Jev is told the page has settled");
}

#[test]
fn a_variable_named_without_its_braces_is_rejected() {
    let flow: Flow = serde_json::from_value(json!({
        "app": "browser",
        "steps": [
            {"pick": {"from": "the flights", "by": "lowest price", "into": "cheapest_flight"}},
            {"verify": "cheapest_flight shows a price"},
            {"verify": "${cheapest_flight} shows a price"},
            {"pick": {"from": "the fares", "by": "lowest price", "into": "fare"}},
            {"verify": "the fare is shown"}
        ]
    }))
    .unwrap();
    let validation = validate::check(&flow, &BTreeSet::new(), &BTreeSet::new());
    assert_eq!(
        validation.errors,
        [
            "step 2: `cheapest_flight` names a variable; write it as `${cheapest_flight}` so its value is shown"
        ],
        "only the bare identifier is an error; a plain word naming a variable is not"
    );
}

#[test]
fn an_oversized_request_is_fitted_under_the_limit() {
    let brief = json!({"goal": "g".repeat(500)});
    let mut questions = ask::Questions::default();
    for group in 0..10 {
        questions = questions.with(
            &format!("group_{group}"),
            ask::options(
                json!({"task": "t", "brief": brief}),
                [("1".to_owned(), json!("a")), ("2".to_owned(), json!("b"))],
            ),
        );
    }
    let state = json!({
        "visible_text": {"untrusted_accessibility_data": (0..400).map(|line| format!("line {line} {}", "x".repeat(40))).collect::<Vec<_>>()},
        "elements": {"untrusted_accessibility_data": (0..50).map(|line| format!("button {line}")).collect::<Vec<_>>()},
    });
    let mut request = ask::request("jev-latest", state, questions);
    let before = serde_json::to_vec(&request).unwrap().len();
    fit(&mut request, 12_000);
    let after = serde_json::to_vec(&request).unwrap().len();
    assert!(before > 12_000 && after <= 12_000, "{before} -> {after}");
    let briefed = request
        .questions
        .values()
        .filter(|question| matches!(question, Question::Choice(choice) if choice.instructions.get("brief").is_some()))
        .count();
    assert_eq!(briefed, 1, "the brief stays on one question");
    let text = request.state["visible_text"]["untrusted_accessibility_data"]
        .as_array()
        .unwrap();
    assert_eq!(
        text[0].as_str().unwrap(),
        format!("line 0 {}", "x".repeat(40)),
        "the top of the screen is kept"
    );
    assert_eq!(
        request.state["elements"]["untrusted_accessibility_data"]
            .as_array()
            .unwrap()
            .len(),
        50
    );

    let mut small = ask::request(
        "jev-latest",
        json!({"a": [1, 2]}),
        ask::Questions::default().with("done", ask::completion("x")),
    );
    let untouched = serde_json::to_value(&small).unwrap();
    fit(&mut small, 12_000);
    assert_eq!(serde_json::to_value(&small).unwrap(), untouched);
    let mut unshrinkable = ask::request(
        "jev-latest",
        json!({"a": "y".repeat(500)}),
        ask::Questions::default().with("done", ask::completion("x")),
    );
    fit(&mut unshrinkable, 100);
    assert_eq!(
        unshrinkable.state["a"].as_str().unwrap().len(),
        500,
        "nothing to cut is left as is"
    );
}

#[tokio::test]
async fn a_long_goal_is_clipped_in_the_brief() {
    let run = run_with(
        App::default(),
        mail_flow(),
        |request| {
            request.brief = tinycomputer_bus::FlowBrief {
                goal: "g".repeat(2000),
                ..tinycomputer_bus::FlowBrief::default()
            };
        },
        |_, _, _| None,
    )
    .await;
    let goal = run
        .requests
        .iter()
        .map(brief_of)
        .find(|brief| !brief.is_null())
        .unwrap()["goal"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(goal.chars().count(), 601);
    assert!(goal.ends_with('…'));
}

fn fare(name: &str, checked: bool) -> Candidate {
    let mut candidate = node(
        name,
        "radio",
        &["Click"],
        &["window", "group \"Fare Types\""],
        300.0,
    );
    if checked {
        candidate.states = vec!["checked".to_owned()];
    }
    candidate
}

#[test]
fn a_fare_card_is_one_option_and_a_checked_one_is_already_chosen() {
    let saver = "Saver fare ₹7,346 + Earn 696 IndiGo BluChips 7 kg Cabin bag allowance 15 kg \
                 Check-in bag allowance Zero change and cancellation charges within 48 hours of \
                 booking Avail Flexi plus fare benefits For just ₹525 Upgrade";
    let flexi = "Flexi plus fare ₹7,871 + Earn 756 IndiGo BluChips 7 kg Cabin bag allowance";
    assert!(!lists_more_than(&fare(saver, false), "Saver"));
    let list = node(saver, "button", &["Click"], &["window"], 1.0);
    assert!(
        lists_more_than(&list, "Saver"),
        "a button this wordy is a list"
    );

    let mut screen = Screen {
        app: "browser".to_owned(),
        window: None,
        surface: "window".to_owned(),
        candidates: vec![fare(saver, true), fare(flexi, false)],
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
    };
    assert_eq!(
        already_chosen(&screen, "Saver").and_then(|chosen| chosen.name),
        Some(saver.to_owned())
    );
    assert!(
        already_chosen(&screen, "Flexi plus").is_none(),
        "the checked card only mentions Flexi plus further in"
    );
    assert!(already_chosen(&screen, "").is_none());
    screen.candidates[0].states.clear();
    assert!(already_chosen(&screen, "Saver").is_none());
}

#[tokio::test]
async fn a_reveal_that_fails_leaves_the_other_ways_to_try() {
    let run = run_with(
        App::quirky(Quirk::Frozen),
        json!({"app": "Mail", "steps": [{"choose": {"what": "the message list", "option": "Message 7"}}]}),
        |_| {},
        |id, question, _| match id {
            "target" => Some(pick(question, "none", 0.9)),
            "move" => Some(pick(question, "activate", 0.9)),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(
        run.result.steps[0].note.contains("was not found"),
        "every way was tried before giving up: {}",
        run.result.steps[0].note
    );
}

#[test]
fn a_search_box_is_searched_by_the_options_name() {
    assert_eq!(steps::search_text("Srinagar (SXR)"), "Srinagar");
    assert_eq!(steps::search_text("Mumbai, BOM"), "Mumbai");
    assert_eq!(steps::search_text("18 October 2026"), "18 October 2026");
    assert_eq!(steps::search_text("(SXR)"), "(SXR)");
}

#[tokio::test]
async fn an_option_already_chosen_is_not_clicked_again() {
    let run = run(
        App::with(|sim| {
            sim.checked_fare = Some("Saver fare ₹7,346 with 15 kg check-in bag allowance and more");
        }),
        json!({"app": "Mail", "steps": [{"choose": {"what": "the fare type", "option": "Saver"}}]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.result.steps[0].outcome, StepOutcome::AlreadyDone);
    assert!(run.app.sim().clicks.is_empty());
    assert!(run.requests.is_empty(), "nothing needed asking");
}

#[tokio::test]
async fn after_acting_a_finished_move_stands_unless_the_judge_leans_undone() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy up"]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(
                question,
                if sim.clicks.is_empty() {
                    "activate"
                } else {
                    "finished"
                },
                0.9,
            )),
            "done" => Some(noul(if sim.clicks.is_empty() { 0.05 } else { 0.6 })),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(
        run.app.sim().clicks.len(),
        1,
        "no click after the step was done"
    );
}

#[tokio::test]
async fn a_date_is_typed_in_the_layout_the_page_asks_for() {
    let run = run(
        App::with(|sim| {
            sim.compose_open = true;
            sim.hint = Some("Please enter the date in (DD-MM-YYYY) format");
        }),
        json!({"app": "Mail", "steps": [{"enter": {"subject": "2000-01-31"}}]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().fields["Subject"], "31-01-2000");
}

#[tokio::test]
async fn a_detail_the_form_does_not_ask_for_is_skipped_not_typed_blindly() {
    let run = run_with(
        App::with(|sim| sim.compose_open = true),
        json!({"app": "Mail", "steps": [{"enter": {"subject": "Hi", "title": "Ms"}}]}),
        |_| {},
        |id, question, _| match id {
            "asks_1" => Some(noul(0.1)),
            _ if id.starts_with("slot_") && text_of(question, "purpose").contains("title") => {
                Some(pick(question, "none", 0.9))
            }
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    let step = &run.result.steps[0];
    assert!(
        step.note.contains("the form does not ask for: title"),
        "{}",
        step.note
    );
    let sim = run.app.sim();
    assert_eq!(sim.fields["Subject"], "Hi", "no blind typing spoiled it");
    assert!(
        !sim.fields.values().any(|value| value.contains("Ms")),
        "{:?}",
        sim.fields
    );
    assert!(
        step.actions
            .iter()
            .all(|action| action.action != "type to filter"),
        "a slot with no field is never typed into the focus"
    );
}

#[tokio::test]
async fn an_option_given_as_a_description_is_matched_by_jev() {
    let run = run_with(
        App::with(|sim| sim.checked_fare = Some("Saver fare ₹7,346 with 15 kg check-in")),
        json!({"app": "Mail", "steps": [
            {"choose": {"what": "the fare types", "option": "the lowest priced fare (e.g. the cheapest one)"}}
        ]}),
        |_| {},
        |id, question, _| {
            (id == "target" && purpose_of(question).contains("that fits"))
                .then(|| pick(question, "Saver", 0.9))
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.result.steps[0].outcome, StepOutcome::AlreadyDone);
    assert!(
        run.app.sim().clicks.is_empty(),
        "the checked fare is left as is"
    );
}

#[test]
fn looks_like_date_rejects_a_day_the_named_month_never_has() {
    assert!(looks_like_date("18 October 2026"));
    // April has 30 days; without a year, February is taken generously (29).
    assert!(!looks_like_date("31 April"));
    assert!(looks_like_date("29 February"));
    // 2026 is not a leap year; 2028 is.
    assert!(!looks_like_date("29 February 2026"));
    assert!(looks_like_date("29 February 2028"));
}

#[test]
fn in_region_prefers_the_ancestor_named_region_but_keeps_every_match_when_none_is_named() {
    let seat = node(
        "Continue",
        "button",
        &["Click"],
        &["root", "Seat picker"],
        10.0,
    );
    let unrelated = node(
        "Continue",
        "button",
        &["Click"],
        &["root", "Newsletter"],
        20.0,
    );
    assert!(in_region(&seat, "seat picker"));
    assert!(!in_region(&unrelated, "seat picker"));
    // An empty `what` names no region to narrow by, so everything matches:
    // `pick_option` falls back to the unnarrowed pool when nothing on the
    // page names the region at all.
    assert!(in_region(&unrelated, ""));
}

#[test]
fn redacted_strips_the_shown_text_but_keeps_the_ref_and_role() {
    let mut target = node("4111 1111 1111 1111", "option", &["Click"], &["root"], 5.0);
    target.description = Some("saved card".to_owned());
    target.value = Some(json!("4111 1111 1111 1111"));
    let logged = redacted(&target);
    assert_eq!(logged.name, None);
    assert_eq!(logged.description, None);
    assert_eq!(logged.value, None);
    assert_eq!(logged.ref_id, target.ref_id);
    assert_eq!(logged.role, target.role);
}

#[tokio::test]
async fn a_first_turn_asks_the_judge_and_the_target_in_one_round_trip() {
    let app = App::default();
    let scratch = std::env::temp_dir().join(format!(
        "tinycomputer-flow-batch-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let runtime = runtime(Oracle {
        app: app.clone(),
        hook: Box::new(activate_moves),
        requests: Mutex::new(Vec::new()),
        fail: false,
    })
    .with_journal(&scratch);
    let request = RunFlowRequest {
        flow: serde_json::from_value(
            json!({"app": "Mail", "steps": [{"open": "Mail"}, "start a new email message"]}),
        )
        .unwrap(),
        votes: 1,
        ..RunFlowRequest::default()
    };
    let reply = super::run_flow(app.clone(), runtime, request).await;
    assert!(reply.ok, "flow run failed: {:?}", reply.error);
    assert_eq!(app.sim().clicks, ["New Message"]);
    let run = std::fs::read_dir(&scratch)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let journal = std::fs::read_to_string(run.join(crate::JOURNAL_FILE)).unwrap();
    let _ = std::fs::remove_dir_all(&scratch);
    let events = journal
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let turns = events
        .iter()
        .filter(|event| event["event"] == "turn")
        .collect::<Vec<_>>();
    assert_eq!(
        (turns[0]["decisions"].as_u64(), turns[0]["rounds"].as_u64()),
        (Some(2), Some(1)),
        "the judge and the target share the first turn's round trip"
    );
    assert_eq!(
        (turns[1]["decisions"].as_u64(), turns[1]["rounds"].as_u64()),
        (Some(1), Some(1)),
        "after acting, the judge is asked alone"
    );
    let batched = events
        .iter()
        .filter(|event| event["event"] == "decision" && event["batched"] == 2)
        .count();
    assert_eq!(batched, 2, "both decisions of the batch say so");
}

#[tokio::test]
async fn a_journaled_run_records_every_exchange_and_what_each_part_took() {
    let app = App::quirky(Quirk::BodyIgnoresSetValue);
    let scratch = std::env::temp_dir().join(format!(
        "tinycomputer-flow-journal-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let runtime = runtime(Oracle {
        app: app.clone(),
        hook: Box::new(|_, _, _| None),
        requests: Mutex::new(Vec::new()),
        fail: false,
    })
    .with_journal(&scratch);
    let request = RunFlowRequest {
        flow: serde_json::from_value(mail_flow()).unwrap(),
        include_values: true,
        votes: 1,
        ..RunFlowRequest::default()
    };
    let reply = super::run_flow(app, runtime, request).await;
    assert!(reply.ok, "flow run failed: {:?}", reply.error);
    let result: FlowRunResult = serde_json::from_value(reply.data.unwrap()).unwrap();

    let runs = std::fs::read_dir(&scratch)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(runs.len(), 1, "one run, one directory");
    let journal = std::fs::read_to_string(runs[0].join(crate::JOURNAL_FILE)).unwrap();
    let _ = std::fs::remove_dir_all(&scratch);
    let events = journal
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let of = |kind: &str| {
        events
            .iter()
            .filter(|event| event["event"] == kind)
            .collect::<Vec<_>>()
    };

    assert_eq!(events[0]["event"], "run");
    assert_eq!(events[0]["kind"], "flow");
    assert_eq!(events[0]["label"], "Mail");
    let end = events.last().unwrap();
    assert_eq!(end["event"], "end");
    assert_eq!(end["stop"], serde_json::to_value(result.stop).unwrap());
    assert!(end["wall_ms"].is_u64());

    let exchanges = of("exchange");
    assert_eq!(
        exchanges.len(),
        usize::try_from(result.metrics.calls).unwrap(),
        "every Jev call is journaled"
    );
    assert!(exchanges.iter().all(|exchange| {
        exchange["ok"] == true
            && exchange["latency_ms"].is_u64()
            && exchange["request"]["questions"].is_object()
            && exchange["answers"].is_object()
    }));
    assert_eq!(exchanges[0]["step"], "2", "exchanges carry their step");
    assert_eq!(
        of("decision").len(),
        exchanges.len(),
        "one framing per decision"
    );
    assert_eq!(of("action").len(), usize::try_from(result.actions).unwrap());
    assert!(of("action").iter().all(|action| action["wall_ms"].is_u64()));
    assert!(
        of("decision")
            .iter()
            .all(|decision| decision["request_bytes"].as_u64().unwrap() > 0)
    );
    let turns = of("turn");
    assert!(!turns.is_empty(), "every do turn is journaled");
    assert!(turns.iter().all(|turn| {
        turn["step"] == "2"
            && turn["decisions"].is_u64()
            && turn["rounds"].is_u64()
            && turn["wall_ms"].is_u64()
    }));
    assert!(!of("observe").is_empty());
    let steps = of("step");
    assert_eq!(
        steps
            .iter()
            .map(|step| step["step"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["1", "2", "3", "4", "5"]
    );
    assert_eq!(steps[4]["outcome"], "gated");
    let seqs = events
        .iter()
        .map(|event| event["seq"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert!(seqs.windows(2).all(|pair| pair[1] == pair[0] + 1));
}

// ---------------------------------------------------------- wide strategy

fn wide(request: &mut RunFlowRequest) {
    request.strategy = tinycomputer_bus::FlowStrategy::Wide;
}

/// Answers `move` with "activate", so a step is done by pressing a control.
fn activate_moves(id: &str, question: &Question, _: &Sim) -> Option<Answer> {
    (id == "move").then(|| pick(question, "activate", 0.9))
}

fn asked(requests: &[EvaluationRequest], id: &str) -> usize {
    requests
        .iter()
        .filter(|request| request.questions.contains_key(id))
        .count()
}

fn asked_prefix(requests: &[EvaluationRequest], prefix: &str) -> usize {
    requests
        .iter()
        .filter(|request| request.questions.keys().any(|id| id.starts_with(prefix)))
        .count()
}

#[tokio::test]
async fn a_wide_turn_judges_and_chooses_its_target_in_one_request() {
    let flow = json!({"app": "Mail", "steps": [{"open": "Mail"}, "start a new email message"]});
    let narrow = run_with(App::default(), flow.clone(), |_| {}, activate_moves).await;
    let broad = run_with(App::default(), flow, wide, activate_moves).await;

    assert_eq!(outcomes(&narrow.result), outcomes(&broad.result));
    assert_eq!(broad.app.sim().clicks, ["New Message"]);
    assert_eq!(
        narrow.result.steps[1].jev_calls, 3,
        "narrow: judge, choose, then judge the result"
    );
    assert_eq!(
        broad.result.steps[1].jev_calls, 2,
        "wide: one request judges the screen and chooses the control, one judges the result"
    );
    let first = &broad.requests[0];
    for id in [
        "done",
        "not_done",
        "progress",
        "move",
        "target_activate",
        "again_activate",
    ] {
        assert!(first.questions.contains_key(id), "the turn asks {id}");
    }
}

#[tokio::test]
async fn a_wide_question_sees_the_screen_as_a_digest_and_the_run_as_memory() {
    let app = App::quirky(Quirk::BodyIgnoresSetValue);
    let run = run_with(app, mail_flow(), wide, |_, _, _| None).await;
    assert_eq!(run.result.stop, FlowStopReason::StoppedBeforeDestructive);

    let entering = run
        .result
        .trace
        .iter()
        .find(|exchange| exchange.step == "3")
        .expect("the enter step asks Jev");
    let state = &entering.state;
    assert!(state.get("recent_actions").is_none());
    assert!(state.get("elements").is_none());
    let regions = &state["screen"]["untrusted_accessibility_data"]["regions"];
    assert!(
        regions
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|region| region["elements"].as_array().unwrap())
            .any(|line| line.as_str().unwrap().contains("textfield \"To\""))
    );
    let memory = &state["memory"];
    assert!(
        memory["steps_done"][1]
            .as_str()
            .unwrap()
            .starts_with("step 2 (do \"start a new email message\"): Done"),
        "{memory}"
    );
    assert_eq!(
        memory["next_step"],
        "verify: the draft shows the recipient, subject and body"
    );
    assert!(memory["budget_left"]["actions"].as_u64().unwrap() > 0);
    for exchange in &run.result.trace {
        assert!(
            !exchange.state["memory"]
                .to_string()
                .contains("Could we move it"),
            "the memory holds what happened, never the text that was typed"
        );
    }
}

#[tokio::test]
async fn a_wide_run_without_the_digest_shows_the_flat_list_with_memory() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| {
            wide(request);
            request.disabled_loops = vec![FlowLoop::Digest];
        },
        |_, _, _| None,
    )
    .await;
    let state = &run.result.trace[0].state;
    assert!(state.get("screen").is_none());
    assert!(state["elements"]["untrusted_accessibility_data"].is_array());
    assert_eq!(state["memory"]["now"], "start a new email message");
}

#[tokio::test]
async fn what_changed_nothing_is_remembered_as_tried_and_failed() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy the inbox"]}),
        wide,
        activate_moves,
    )
    .await;
    assert_eq!(run.result.steps[0].outcome, StepOutcome::Failed);
    let later = run
        .result
        .trace
        .iter()
        .find(|exchange| exchange.state["memory"].get("tried_and_failed").is_some())
        .expect("a later turn is told what did not work");
    assert_eq!(
        later.state["memory"]["tried_and_failed"][0],
        "pressed button \"Archive\": nothing on screen changed"
    );
}

#[tokio::test]
async fn an_obstacle_in_front_is_cleared_from_the_turns_own_request_and_remembered() {
    let app = App::with(|sim| sim.obstacle = true);
    let run = run_with(
        app,
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        wide,
        |_, _, _| None,
    )
    .await;
    assert_eq!(run.result.steps[0].outcome, StepOutcome::Done);
    let clicks = run.app.sim().clicks.clone();
    assert_eq!(clicks, ["Keep Editing"]);
    let Question::Choice(dismiss) = &run.requests[0].questions["dismiss"] else {
        panic!("dismiss is a choice");
    };
    assert!(
        !serde_json::to_string(&dismiss.criteria)
            .unwrap()
            .contains("Delete Draft"),
        "an irreversible control is never offered to dismiss with"
    );
    assert_eq!(asked(&run.requests, "dismiss"), 1);
    assert!(
        run.requests
            .iter()
            .filter(|request| request.questions.contains_key("dismiss"))
            .all(|request| request.questions.contains_key("done")),
        "the obstacle is asked about in the turn's own request, never alone"
    );
    let hint = run
        .result
        .learned
        .iter()
        .find(|hint| hint.key == "obstacle sheet")
        .expect("the control that closed the sheet is remembered");
    assert_eq!(hint.name.as_deref(), Some("Keep Editing"));

    let again = run_with(
        App::with(|sim| sim.obstacle = true),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| {
            wide(request);
            request.memory = vec![hint.clone()];
        },
        // Only the remembered control is confirmed; the chooser picks
        // nothing, so the dismissal can only come from memory.
        |id, question, _| (id == "dismiss").then(|| pick(question, "none", 0.9)),
    )
    .await;
    assert_eq!(asked(&again.requests, "dismiss_known"), 1);
    assert_eq!(again.app.sim().clicks, ["Keep Editing"]);
}

#[tokio::test]
async fn dismiss_options_honor_the_runs_value_visibility_policy() {
    let hidden = run_with(
        App::with(|sim| sim.obstacle = true),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| {
            wide(request);
            request.include_values = false;
        },
        |_, _, _| None,
    )
    .await;
    let Question::Choice(dismiss) = &hidden.requests[0].questions["dismiss"] else {
        panic!("dismiss is a choice");
    };
    assert!(
        !serde_json::to_string(&dismiss.criteria)
            .unwrap()
            .contains("unsaved-draft-42"),
        "a held value is hidden from dismiss options when include_values is false"
    );

    let shown = run_with(
        App::with(|sim| sim.obstacle = true),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| {
            wide(request);
            request.include_values = true;
        },
        |_, _, _| None,
    )
    .await;
    let Question::Choice(dismiss) = &shown.requests[0].questions["dismiss"] else {
        panic!("dismiss is a choice");
    };
    assert!(
        serde_json::to_string(&dismiss.criteria)
            .unwrap()
            .contains("unsaved-draft-42"),
        "a held value must reach dismiss options when the run allows include_values"
    );
}

#[tokio::test]
async fn a_crowded_screen_is_surveyed_once_and_ranked_instead_of_narrowed() {
    let app = App::with(|sim| sim.extra_buttons = 60);
    let run = run_with(
        app,
        json!({"app": "Mail", "steps": ["open message 7"]}),
        wide,
        |id, question, sim| {
            if id == "done" {
                return Some(noul(if sim.clicks.contains(&"Message 7".to_owned()) {
                    0.95
                } else {
                    0.05
                }));
            }
            activate_moves(id, question, sim)
        },
    )
    .await;
    assert_eq!(run.result.steps[0].outcome, StepOutcome::Done);
    assert_eq!(run.app.sim().clicks, ["Message 7"]);
    assert_eq!(
        asked_prefix(&run.requests, "relevance_"),
        1,
        "one survey for the step's page shape, reused on later turns"
    );
    assert_eq!(
        asked(&run.requests, "region"),
        0,
        "no region-by-region narrowing"
    );
    assert!(run.result.steps[0].loops.contains(&FlowLoop::Survey));

    let survey = run
        .requests
        .iter()
        .find(|request| {
            request
                .questions
                .keys()
                .any(|id| id.starts_with("relevance_"))
        })
        .unwrap();
    assert!(
        survey
            .questions
            .keys()
            .any(|id| id.starts_with("distraction_"))
    );
    let turn = run
        .requests
        .iter()
        .find(|request| request.questions.contains_key("done"))
        .unwrap();
    let Question::Choice(first_group) = &turn.questions["group_activate_0"] else {
        panic!("a crowded pool is knocked out in the turn's own request");
    };
    let first_option = first_group.criteria["1"].as_ref().unwrap().to_string();
    assert!(
        first_option.contains("Region 1"),
        "the region the survey ranked highest is offered first: {first_option}"
    );
    let regions = &turn.state["screen"]["untrusted_accessibility_data"]["regions"];
    assert_eq!(regions[0]["relevance"], 1.0);
}

#[tokio::test]
async fn a_hesitant_wide_pick_is_confirmed_before_it_is_pressed() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        wide,
        |id, question, sim| {
            if id == "target_activate" {
                return Some(pick(question, "New Message", 0.5));
            }
            activate_moves(id, question, sim)
        },
    )
    .await;
    assert_eq!(run.app.sim().clicks, ["New Message"]);
    let confirmations = run
        .requests
        .iter()
        .filter(|request| request.questions.keys().collect::<Vec<_>>() == ["confirm"])
        .count();
    assert_eq!(confirmations, 1, "one yes/no settles the hesitant pick");

    let refused = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| {
            wide(request);
            request.disabled_loops = vec![FlowLoop::Consistency, FlowLoop::Corroboration];
        },
        |id, question, sim| {
            if id == "target_activate" {
                return Some(pick(question, "New Message", 0.5));
            }
            activate_moves(id, question, sim)
        },
    )
    .await;
    assert!(
        refused.app.sim().clicks.is_empty(),
        "with nothing to check a hesitant pick against, it is not pressed"
    );
}

#[tokio::test]
async fn a_journaled_wide_run_records_its_survey_and_each_turns_decisions() {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let app = App::with(|sim| sim.extra_buttons = 60);
    let scratch = std::env::temp_dir().join(format!(
        "tinycomputer-wide-journal-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let runtime = runtime(Oracle {
        app: app.clone(),
        hook: Box::new(|_, _, _| None),
        requests: Mutex::new(Vec::new()),
        fail: false,
    })
    .with_journal(&scratch);
    let request = RunFlowRequest {
        flow: serde_json::from_value(
            json!({"app": "Mail", "steps": ["start a new email message"]}),
        )
        .unwrap(),
        votes: 1,
        strategy: tinycomputer_bus::FlowStrategy::Wide,
        ..RunFlowRequest::default()
    };
    let reply = super::run_flow(app, runtime, request).await;
    assert!(reply.ok, "flow run failed: {:?}", reply.error);
    let run = std::fs::read_dir(&scratch)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let events = std::fs::read_to_string(run.join(crate::JOURNAL_FILE))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    std::fs::remove_dir_all(&scratch).unwrap();
    let survey = events
        .iter()
        .find(|event| event["event"] == "survey")
        .expect("the crowded screen was surveyed");
    assert_eq!(survey["step"], "1");
    assert!(survey["regions"].as_u64().unwrap() >= 2);
    assert!(survey["most_relevant"][0].is_string());
    assert_eq!(survey["distractions"], 0);
    let turns = events
        .iter()
        .filter(|event| event["event"] == "turn")
        .collect::<Vec<_>>();
    assert_eq!(
        turns[0]["decisions"], 2,
        "the first turn: the survey and one wide request"
    );
}

#[tokio::test]
async fn the_memory_keeps_the_last_steps_actions_as_evidence_for_the_next() {
    // Live, a payment page was judged "seat selection skipped" at 0.48
    // until the previous step's clicks were back in view (0.95): the click
    // that left a page is the evidence it was dealt with.
    let flow = json!({"app": "Mail", "steps": [
        "start a new email message",
        {"verify": "a new message is open"}
    ]});
    let run = run_with(App::default(), flow, wide, activate_moves).await;
    let verifying = run
        .result
        .trace
        .iter()
        .find(|exchange| exchange.step == "2")
        .unwrap();
    let recent = verifying.state["memory"]["recent_actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(
        recent
            .iter()
            .any(|line| line.starts_with("click button \"New Message\"")),
        "{recent:?}"
    );
    assert!(
        recent
            .iter()
            .any(|line| line.starts_with("after the last action: window is now \"New Message\"")),
        "{recent:?}"
    );
}

#[tokio::test]
async fn a_target_answered_none_is_not_asked_again_the_same_turn() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        wide,
        |id, question, sim| {
            if id == "target_activate" {
                return Some(pick(question, "none", 0.95));
            }
            activate_moves(id, question, sim)
        },
    )
    .await;
    assert_eq!(asked(&run.requests, "target"), 0, "no narrow re-ask");
    assert!(run.app.sim().clicks.is_empty());
    assert_eq!(run.result.steps[0].outcome, StepOutcome::Failed);
}

#[tokio::test]
async fn a_gated_stop_before_asks_to_find_the_control_not_to_press_it() {
    let run = run(App::default(), mail_flow()).await;
    assert_eq!(run.result.stop, FlowStopReason::StoppedBeforeDestructive);
    let purposes = run
        .requests
        .iter()
        .filter_map(|request| request.questions.get("target"))
        .map(|question| text_of(question, "purpose"))
        .collect::<Vec<_>>();
    assert!(
        purposes.iter().any(|purpose| purpose
            == "find, without pressing it, the control that would perform: sending the email"),
        "{purposes:?}"
    );

    let remembered = run_with(
        App::default(),
        mail_flow(),
        |request| {
            request.memory = vec![GroundingHint {
                app: "Mail".to_owned(),
                key: "perform sending the email".to_owned(),
                role: "button".to_owned(),
                name: Some("Send".to_owned()),
                path: vec!["window \"New Message\"".to_owned(), "toolbar".to_owned()],
            }];
        },
        |_, _, _| None,
    )
    .await;
    assert_eq!(
        remembered.result.stop,
        FlowStopReason::StoppedBeforeDestructive
    );
    assert!(
        remembered
            .requests
            .iter()
            .any(|request| request.questions.keys().collect::<Vec<_>>() == ["confirm"]),
        "a hint stored under the step's own words is still recalled: {:?}",
        remembered
            .requests
            .iter()
            .map(|r| r.questions.keys().cloned().collect::<Vec<_>>())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn a_picked_card_whose_control_is_covered_is_uncovered_and_opened() {
    // Live on Google Flights the cheapest card's "Select flight" link was
    // refused as covered and the pick failed; it now presses Escape and
    // tries the same card once more, as a `do` step's click does.
    let app = flights();
    app.sim().quirks.insert(Quirk::Drawer);
    let run = run(
        app,
        json!({"app": "Mail", "steps": [
            {"pick": {"from": "the flight results", "by": "lowest price"}}
        ]}),
    )
    .await;
    assert_eq!(run.result.steps[0].outcome, StepOutcome::Done);
    let sim = run.app.sim();
    assert_eq!(sim.presses, ["escape"]);
    assert_eq!(sim.picked, ["@s:select-1"]);
}

#[tokio::test]
async fn a_picked_card_that_cannot_be_opened_says_why() {
    let app = flights();
    app.sim().quirks.insert(Quirk::Unclickable);
    let run = run(
        app,
        json!({"app": "Mail", "steps": [
            {"pick": {"from": "the flight results", "by": "lowest price"}}
        ]}),
    )
    .await;
    let step = &run.result.steps[0];
    assert_eq!(step.outcome, StepOutcome::Failed);
    assert!(
        step.note.starts_with(
            "could not open the picked item (Element exists but is not visible.): IndiGo"
        ),
        "{}",
        step.note
    );
    assert!(
        run.app.sim().presses.is_empty(),
        "only a covered click is retried"
    );
}

#[test]
fn the_control_pressed_last_turn_is_offered_last_even_with_new_states() {
    let toggle = node(
        "Destination",
        "button",
        &["Click"],
        &["window \"Book\"", "form"],
        10.0,
    );
    let other = node(
        "Srinagar",
        "option",
        &["Click"],
        &["window \"Book\"", "listbox"],
        20.0,
    );
    let mut pool = vec![
        Candidate {
            states: vec!["expanded".to_owned()],
            ..toggle.clone()
        },
        other.clone(),
    ];
    super::wide::pressed_last(&mut pool, Some(&toggle));
    assert_eq!(pool[0].name.as_deref(), Some("Srinagar"));
    let mut grown = vec![
        Candidate {
            name: Some("Destination POPULAR DESTINATIONS Mumbai".to_owned()),
            ..toggle.clone()
        },
        other.clone(),
    ];
    super::wide::pressed_last(&mut grown, Some(&toggle));
    assert_eq!(
        grown[0].name.as_deref(),
        Some("Srinagar"),
        "a button whose name grew by the list it opened is the same button"
    );
    assert_eq!(pool[1].name.as_deref(), Some("Destination"));

    let mut untouched = vec![toggle.clone(), other];
    super::wide::pressed_last(&mut untouched, None);
    assert_eq!(untouched[0].name.as_deref(), Some("Destination"));
}

#[test]
fn survey_answers_follow_a_region_by_name_when_its_id_moves() {
    let form = node(
        "From",
        "textbox",
        &["Click"],
        &["webarea \"Book\"", "form \"Search\""],
        1.0,
    );
    let ad = node(
        "Deal",
        "link",
        &["Click"],
        &["webarea \"Book\"", "region \"Offers\""],
        2.0,
    );
    let before = Screen {
        app: "IndiGo".to_owned(),
        window: None,
        surface: "window".to_owned(),
        candidates: vec![form.clone(), ad.clone()],
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
    };
    let digest = super::view::digest(&before);
    let mut attention = super::survey::Attention::default();
    attention
        .relevance
        .insert(digest.region_of(0).unwrap().name.clone(), 0.9);
    attention
        .distractions
        .insert(digest.region_of(1).unwrap().name.clone());

    // A dropdown opens above both: every region's id moves on by one.
    let mut after = before.clone();
    after.candidates.insert(
        0,
        node(
            "Srinagar",
            "option",
            &["Click"],
            &["webarea \"Book\"", "listbox"],
            0.0,
        ),
    );
    let moved = super::view::digest(&after);
    let (relevance, distractions) = attention.for_digest(&moved);
    let form_id = moved.region_of(1).unwrap().id.clone();
    let ad_id = moved.region_of(2).unwrap().id.clone();
    assert_eq!(relevance.get(&form_id), Some(&0.9));
    assert!(distractions.contains(&ad_id));
    assert_eq!(relevance.len(), 1, "the new listbox has no answer yet");
}

#[test]
fn lookalikes_are_offered_once_and_the_first_in_page_order_is_kept() {
    let search = |order: usize, y: f64| Candidate {
        ref_id: format!("@s:search-{order}"),
        role: "combobox".to_owned(),
        available_actions: vec!["SetValue".to_owned()],
        bounds: Some(json!({"x": 10.0, "y": y})),
        path: vec!["main".to_owned(), "button \"destinationCity\"".to_owned()],
        order,
        ..Candidate::default()
    };
    let named = node("Pax Selection", "combobox", &["SetValue"], &["main"], 5.0);
    let pool = super::view::distinct(
        vec![
            search(1, 10.0),
            named.clone(),
            search(2, 20.0),
            search(3, 0.0),
        ],
        false,
    );
    let refs = pool.iter().map(|c| c.ref_id.as_str()).collect::<Vec<_>>();
    assert_eq!(refs, ["@s:search-1", "@s:Pax Selection"]);
}

#[tokio::test]
async fn a_field_that_refuses_the_text_is_struck_and_the_real_one_is_used() {
    // A page gives each suggested city a `combobox` role and no name, each
    // holding its city as a value; Jev takes one for "the destination
    // search", as it did live. It refuses the text, and the step must strike
    // every row of its kind, not try them one by one.
    let run = run_with(
        App::quirky(Quirk::CityRows),
        json!({"app": "Mail", "steps": [{"enter": {"destination search": "Srinagar"}}]}),
        |_| {},
        |id, question, _| {
            id.starts_with("slot_").then(|| {
                let offered = serde_json::to_string(question).unwrap();
                let needle = if offered.contains(r#""what":"combobox""#) {
                    r#""what":"combobox""#
                } else {
                    "Search"
                };
                pick(question, needle, 0.9)
            })
        },
    )
    .await;
    let step = &run.result.steps[0];
    assert_eq!(step.outcome, StepOutcome::Done, "{}", step.note);
    let fills = step
        .actions
        .iter()
        .map(|action| (action.target.as_ref().unwrap().ref_id.clone(), action.ok))
        .collect::<Vec<_>>();
    assert_eq!(
        fills,
        [
            ("@s:city-0".to_owned(), false),
            ("@s:Search".to_owned(), true)
        ],
        "one refusal strikes every row of its kind, then the real field"
    );
    assert_eq!(run.app.sim().fields["Search"], "Srinagar");
}

#[tokio::test]
async fn a_row_that_refused_the_text_is_never_pressed_while_revealing_a_field() {
    // Live, after a city row refused the text, the reveal loop pressed that
    // row and chose a city nobody asked for.
    let run = run_with(
        App::quirky(Quirk::CityRows),
        json!({"app": "Mail", "steps": [{"enter": {"destination search": "Srinagar"}}]}),
        |_| {},
        |id, question, _| {
            let offered = serde_json::to_string(question).unwrap();
            let row = r#""what":"combobox""#;
            match id {
                // Jev keeps wanting a row: for the text, and to reveal.
                _ if id.starts_with("slot_") || id == "target" => Some(if offered.contains(row) {
                    pick(question, row, 0.9)
                } else {
                    pick(question, "none", 0.9)
                }),
                "move" => Some(pick(question, "activate", 0.9)),
                _ => None,
            }
        },
    )
    .await;
    let step = &run.result.steps[0];
    let rows = step
        .actions
        .iter()
        .filter(|action| {
            action
                .target
                .as_ref()
                .is_some_and(|target| target.ref_id.starts_with("@s:city-"))
        })
        .map(|action| action.action.clone())
        .collect::<Vec<_>>();
    assert_eq!(rows, ["fill destination search"], "{:?}", step.actions);
    assert_eq!(step.outcome, StepOutcome::Failed);
    assert_eq!(
        step.note,
        "no field that takes text was found for: destination search; 1 element(s) the page offered as fields refused the text"
    );
}
