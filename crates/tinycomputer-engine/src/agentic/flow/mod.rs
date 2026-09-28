//! High-level intent flows run by Jev decision loops.
//!
//! A [`Flow`] says *what* to accomplish, step by step, with no UI knowledge.
//! This module grounds each step on the live screen by composing many small
//! Jev questions in deterministic Rust:
//!
//! - `ask` builds the questions: completion and condition Nouls, a progress
//!   Score, an obstacle Noul, and element Choices of at most 20 options.
//! - `ground` picks one element for a purpose: grounding memory first, then
//!   region-by-region narrowing, then a relabelled re-ask and a yes/no
//!   corroboration when the first answer is not confident.
//! - `act` is the loop behind a `do` step: judge, choose an app-agnostic move,
//!   act, and judge again, with obstacle dismissal and undo.
//! - `enter` matches slots to fields in one request and delivers each text
//!   with read-back verification.
//! - `steps` implements the remaining step kinds.
//! - `memory` remembers which element grounded which step.
//! - `vote` asks each decision several ways at once and averages the answers.
//! - `wide` is the wide strategy: one request per `do` turn over a digest of
//!   the screen, carrying the judgement, the obstacle, and every move's
//!   target; `survey` ranks a crowded screen's regions first, and `ledger`
//!   is the working memory every wide question sees.
//! - Deliberation (`docs/specs/jev-deliberation.md`) decides on evidence
//!   rather than one probability: `evidence` reads a question's ballot into
//!   accept, deliberate, or abstain; `escalate` asks a close call more ways,
//!   `duel` settles close candidates two at a time; `denoise` ranks what is
//!   in view first; `expect` checks an action did what it should; and
//!   `checkpoint` undoes a mistake and verifies the undo.
//!
//! Every request carries the run's brief — the goal, whom it is for, the
//! plan and where the run is in it, what it has chosen so far, and what kind
//! of page is showing — so each small decision is made knowing the whole
//! task. Secrets never leave as values: every request is masked so a secret
//! reads `${name}` wherever it would have appeared.
//!
//! See `docs/specs/jev-intent-flows.md` for the design and its rationale.

mod act;
mod ask;
mod attention;
mod backend;
mod checkpoint;
mod denoise;
mod duel;
mod enter;
mod escalate;
mod evidence;
mod expect;
mod ground;
mod ledger;
mod memory;
mod reflect;
mod steps;
mod survey;
mod validate;
mod view;
mod vote;
mod wide;

pub(crate) use validate::{check as check_flow, missing_inputs};

#[cfg(test)]
mod test;

use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    time::Instant,
};

use serde_json::{Value, json};
use tinycomputer_bus::{
    Deliberation, DesktopError, DesktopResponse, FLOW_GUIDE, Flow, FlowAction, FlowActionRecord,
    FlowBrief, FlowLoop, FlowRunResult, FlowStep, FlowStopReason, FlowStrategy, GroundingHint,
    JevExchange, JevMetrics, JevTarget, RunFlowRequest, StepOutcome, StepReport,
    ValidateFlowRequest,
};
use tinycomputer_core::Facts;
use tinyinference_decisions::{Answer, EvaluationRequest, Question};

use super::journal::millis;
use super::{JevRuntime, merge_metrics, provider_error, response};
use backend::{AgentBackend, blocking, observe_async};
use validate::{step_path, substitute_safe};
use view::{Candidate, Depth, Screen, target_payload};

/// Upper bound on [`RunFlowRequest::max_actions`].
const MAX_ACTIONS: u32 = 120;
/// Upper bound on [`RunFlowRequest::max_model_calls`]. Jev is cheap, and
/// every framing of a voted decision is one evaluation.
const MAX_CALLS: u32 = 10_000;
/// Choices, picks, and entries remembered for the brief's `so_far`.
const MAX_SO_FAR: usize = 12;
/// Longest goal the brief carries, in characters.
const MAX_GOAL: usize = 600;
/// Longest plan line the brief carries, in characters.
const MAX_PLAN_LINE: usize = 120;
/// Longest `so_far` note the brief carries, in characters.
const MAX_SO_FAR_NOTE: usize = 200;
/// Largest request sent to Jev, in bytes of JSON. Jev refuses one past its
/// token limit outright (HTTP 400, `max_tokens_exceeded`), which ends the
/// run; measured, 120 KB passed and 160 KB did not.
const MAX_REQUEST_BYTES: usize = 100_000;
/// Consecutive unreadable observations that fail a step.
const MAX_BLIND_LOOKS: u32 = 3;
/// Truncated subtrees one exploration reads at most.
const MAX_EXPLORED: usize = 4;

/// Runs `request` on `surface`: a `Desktop`, or a
/// [`Workspace`](crate::Workspace) joining the desktop and the browser.
pub async fn run_flow<S: AgentBackend + Sync>(
    surface: S,
    runtime: JevRuntime,
    request: RunFlowRequest,
) -> DesktopResponse {
    let label = if request.brief.goal.is_empty() {
        request.flow.app.clone()
    } else {
        format!("{}: {}", request.flow.app, request.brief.goal)
    };
    let runtime = runtime.begin_run("flow", &label);
    run_flow_with(surface, &runtime, request).await
}

/// Checks a flow without touching the desktop or Jev.
#[must_use]
pub fn validate_flow(request: &ValidateFlowRequest) -> DesktopResponse {
    let (_, validation) = validate::validate(&request.flow, &BTreeSet::new(), &BTreeSet::new());
    response("validate-flow", &validation)
}

/// The flow authoring guide, as prompt text.
#[must_use]
pub fn flow_guide() -> DesktopResponse {
    DesktopResponse::ok(
        "flow-guide",
        json!({"guide": FLOW_GUIDE, "step_kinds": tinycomputer_bus::STEP_KINDS}),
    )
}

pub(super) async fn run_flow_with<B: AgentBackend + Sync>(
    backend: B,
    runtime: &JevRuntime,
    request: RunFlowRequest,
) -> DesktopResponse {
    let known = request.vars.keys().cloned().collect::<BTreeSet<_>>();
    let validation = validate::check(&request.flow, &known, &request.facts);
    if !validation.valid {
        return DesktopResponse::err(
            "run-flow",
            DesktopError::new("FLOW_INVALID", validation.errors.join("; ")),
        );
    }
    let flow = request.flow.clone();
    let mut run = FlowRun::new(backend, runtime, &request);
    let stop = match run.start(&flow).await {
        Ok(()) => FlowStopReason::Completed,
        Err(Halt::Stop(stop)) => stop,
        Err(Halt::Error(error)) => return *error,
        Err(Halt::Failed(_)) => FlowStopReason::StepFailed,
    };
    run.finish(stop)
}

/// Why a run stopped early.
#[derive(Debug)]
pub(super) enum Halt {
    /// The run stops with a structured reason.
    Stop(FlowStopReason),
    /// A provider failure the caller must see as an error envelope.
    Error(Box<DesktopResponse>),
    /// The current step failed; the note says why.
    Failed(String),
}

/// What one step spent and did, accumulated while it runs.
#[derive(Debug, Default)]
pub(super) struct StepLog {
    pub(super) calls: u32,
    pub(super) turns: u32,
    pub(super) actions: Vec<FlowActionRecord>,
    pub(super) loops: BTreeSet<FlowLoop>,
    pub(super) confidence: Option<f64>,
    /// Whether the step typed to filter a list: the option it then pressed
    /// should leave the list, or show selected (`steps::left_unchosen`).
    pub(super) filtered: bool,
}

impl StepLog {
    pub(super) fn used(&mut self, flow_loop: FlowLoop) {
        self.loops.insert(flow_loop);
    }
}

/// How a step ended, and why.
#[derive(Debug)]
pub(super) struct Ended {
    pub(super) outcome: StepOutcome,
    pub(super) note: String,
}

impl Ended {
    pub(super) fn new(outcome: StepOutcome, note: impl Into<String>) -> Self {
        Self {
            outcome,
            note: note.into(),
        }
    }
}

/// State of one flow run.
pub(super) struct FlowRun<'r, B> {
    pub(super) backend: B,
    runtime: &'r JevRuntime,
    pub(super) app: String,
    pub(super) vars: BTreeMap<String, String>,
    /// Names among `vars` that are the task's facts: never expanded into any
    /// text a Jev evaluation sees, as a runtime backstop behind validation.
    pub(super) facts: BTreeSet<String>,
    pub(super) allow_destructive: bool,
    pub(super) include_values: bool,
    max_actions: u32,
    max_calls: u32,
    disabled: BTreeSet<FlowLoop>,
    pub(super) memory: Vec<GroundingHint>,
    pub(super) learned: Vec<GroundingHint>,
    pub(super) history: Vec<String>,
    metrics: JevMetrics,
    actions: u32,
    reports: Vec<StepReport>,
    pub(super) pending: Option<JevTarget>,
    blind_looks: u32,
    tracing: bool,
    trace: Vec<JevExchange>,
    /// When the run began, for the journal's end-of-run wall time.
    started: Instant,
    step: String,
    /// What the run is for, shown with every question.
    brief: FlowBrief,
    /// The flow's top-level steps as the brief lists them.
    outline: Vec<String>,
    /// What the run has chosen, picked, and entered so far, newest last.
    pub(super) so_far: Vec<String>,
    /// The kind of page last seen, by Jev's reading of it.
    pub(super) page: Option<String>,
    /// How many ways each decision is asked.
    votes: u32,
    /// The secret values, to mask every request with.
    secrets: Facts,
    /// Every `stop_before` phrase the flow declares, gathered once up front
    /// so an ordinary step's destructive gate can recognize a control the
    /// flow has already named as irreversible, in its own words.
    pub(super) stop_before: Vec<String>,
    /// How decisions are asked.
    strategy: FlowStrategy,
    /// The run's working memory, for the wide strategy's questions.
    ledger: ledger::Ledger,
    /// The current step's survey of a crowded screen, if one was asked.
    attention: Option<survey::Attention>,
    /// Decisions made so far: each one request, whatever its framings.
    decisions: u32,
    /// Round trips to Jev: a batch of decisions asked at once is one.
    rounds: u32,
    /// Variables read from the screen so far, by name.
    read: Vec<String>,
    /// Kinds of element (`view::element_kind`) that refused text in this
    /// step: options in a list, never pressed by a `do` move while the step
    /// looks for somewhere to type.
    pub(super) refused: BTreeSet<String>,
    /// Kinds of element (`view::element_kind`) this run typed into: a
    /// field holding text the flow typed shows no choice the page made
    /// (`steps::already_holds`).
    pub(super) typed: BTreeSet<String>,
    /// How much the run deliberates before acting on a decision.
    deliberation: Deliberation,
    /// Every framing's own answer to each question, under the original
    /// keys, from the latest decision that asked it: the evidence a
    /// deliberating decision reads (`evidence.rs`) and widens (`escalate`).
    ballots: BTreeMap<String, Vec<tinyinference_decisions::Answer>>,
    /// The address the surface last reported, on a surface that has them:
    /// a checkpoint's location, and how a navigation is noticed.
    pub(super) location: Option<String>,
    /// The runners-up of the step's latest grounding, best first: the
    /// branches a backtrack tries next (`checkpoint.rs`).
    pub(super) frontier: Vec<Candidate>,
    /// The last press and what it was meant to do, while the turn after it
    /// is judged: the judge then asks whether it did (`expect.rs`).
    pub(super) expecting: Option<(String, String)>,
    /// The address the current step began at, to return to when the step
    /// is found to have gone wrong.
    pub(super) step_location: Option<String>,
}

/// Every `stop_before` phrase in `steps`, gathered from every branch of
/// `if` and every round of `repeat_until`: a control the flow names there is
/// irreversible regardless of which branch a run actually takes.
fn stop_before_phrases(steps: &[FlowStep]) -> Vec<String> {
    let mut phrases = Vec::new();
    collect_stop_before(steps, &mut phrases);
    phrases
}

fn collect_stop_before(steps: &[FlowStep], phrases: &mut Vec<String>) {
    for step in steps {
        match step.action() {
            FlowAction::StopBefore(phrase) => phrases.push(phrase),
            FlowAction::RepeatUntil(repeat) => collect_stop_before(&repeat.steps, phrases),
            FlowAction::If(branch) => {
                collect_stop_before(&branch.then, phrases);
                collect_stop_before(&branch.otherwise, phrases);
            }
            _ => {}
        }
    }
}

impl<'r, B: AgentBackend + Sync> FlowRun<'r, B> {
    fn new(backend: B, runtime: &'r JevRuntime, request: &RunFlowRequest) -> Self {
        // A flow's own definitions may name the caller's values
        // (`"first_name": "${first name}"`), so they are expanded once
        // against them. The caller's values are never rescanned: one that
        // happens to contain `${…}` stays as written.
        let mut vars = request
            .flow
            .vars
            .iter()
            .map(|(name, value)| (name.clone(), validate::substitute(value, &request.vars)))
            .collect::<BTreeMap<_, _>>();
        vars.extend(request.vars.clone());
        let facts = validate::carrying_facts(&request.flow.vars, &request.facts);
        let secrets = Facts::with_secrets(
            facts
                .iter()
                .filter_map(|name| Some((name.clone(), vars.get(name)?.clone()))),
            facts
                .iter()
                .filter(|name| vars.contains_key(*name))
                .cloned(),
        )
        .unwrap_or_default();
        let outline = request
            .flow
            .steps
            .iter()
            .map(|step| {
                let (kind, text) = describe_step(&step.action(), &vars, &facts);
                format!("{kind}: {text}")
            })
            .collect();
        Self {
            backend,
            runtime,
            app: request.flow.app.clone(),
            stop_before: stop_before_phrases(&request.flow.steps),
            vars,
            // A flow variable defined from a secret now holds that secret's
            // value, so it is kept out of model-facing text the same way.
            facts,
            brief: request.brief.clone(),
            outline,
            so_far: Vec::new(),
            page: None,
            votes: request.votes.clamp(1, vote::MAX_VOTES),
            secrets,
            allow_destructive: request.allow_destructive,
            include_values: request.include_values,
            max_actions: request.max_actions.min(MAX_ACTIONS),
            max_calls: request.max_model_calls.min(MAX_CALLS),
            disabled: request
                .disabled_loops
                .iter()
                .copied()
                .filter(|flow_loop| *flow_loop != FlowLoop::Slots)
                .collect(),
            memory: request.memory.clone(),
            learned: Vec::new(),
            history: Vec::new(),
            metrics: JevMetrics::default(),
            actions: 0,
            reports: Vec::new(),
            pending: None,
            blind_looks: 0,
            tracing: request.trace,
            trace: Vec::new(),
            started: Instant::now(),
            step: String::new(),
            strategy: request.strategy,
            ledger: ledger::Ledger::default(),
            attention: None,
            decisions: 0,
            rounds: 0,
            read: Vec::new(),
            refused: BTreeSet::new(),
            typed: BTreeSet::new(),
            deliberation: request.deliberation,
            ballots: BTreeMap::new(),
            location: None,
            frontier: Vec::new(),
            expecting: None,
            step_location: None,
        }
    }

    async fn start(&mut self, flow: &Flow) -> Result<(), Halt> {
        let app = self.app.clone();
        let mut log = StepLog::default();
        self.act(&mut log, "launch", None, move |backend| {
            backend.launch(&app)
        })
        .await?;
        self.run_steps(&flow.steps, String::new()).await
    }

    /// Runs `steps` in order, recursing into `if` and `repeat_until`.
    pub(super) fn run_steps<'s>(
        &'s mut self,
        steps: &'s [FlowStep],
        prefix: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), Halt>> + Send + 's>> {
        Box::pin(async move {
            for (index, step) in steps.iter().enumerate() {
                self.run_step(step, step_path(&prefix, index)).await?;
            }
            Ok(())
        })
    }

    async fn run_step(&mut self, step: &FlowStep, path: String) -> Result<(), Halt> {
        let action = step.action();
        let (kind, text) = describe_step(&action, &self.vars, &self.facts);
        let mut log = StepLog::default();
        self.begin_step(&path);
        let started = Instant::now();
        let result = steps::run(self, &mut log, &action, &text, &path).await;
        let result = self.reflected(&mut log, &action, &text, result).await;
        let wall_ms = millis(started.elapsed());
        let (ended, halt) = match result {
            Ok(ended) => (ended, None),
            Err(Halt::Failed(note)) => {
                let ended = Ended::new(StepOutcome::Failed, note.clone());
                (ended, Some(Halt::Failed(note)))
            }
            Err(Halt::Stop(reason)) => {
                let outcome = if reason == FlowStopReason::StoppedBeforeDestructive {
                    StepOutcome::Gated
                } else {
                    StepOutcome::Failed
                };
                let note = match reason {
                    FlowStopReason::StoppedBeforeDestructive => {
                        "stopped in front of the irreversible action".to_owned()
                    }
                    FlowStopReason::ActionBudget => "the action budget ran out".to_owned(),
                    FlowStopReason::ModelBudget => "the Jev call budget ran out".to_owned(),
                    _ => format!("stopped: {reason:?}"),
                };
                (Ended::new(outcome, note), Some(Halt::Stop(reason)))
            }
            Err(error @ Halt::Error(_)) => return Err(error),
        };
        let text = self.secrets.mask(&text);
        let ended = Ended::new(ended.outcome, self.secrets.mask(&ended.note));
        self.step.clone_from(&path);
        self.runtime.journal.record("step", || {
            json!({
                "step": path,
                "kind": kind,
                "text": text,
                "outcome": ended.outcome,
                "note": ended.note,
                "turns": log.turns,
                "jev_calls": log.calls,
                "actions": log.actions.len(),
                "loops": log.loops,
                "confidence": log.confidence,
                "wall_ms": wall_ms,
            })
        });
        let finished = format!(
            "step {path} ({kind} {text:?}): {:?}, {}",
            ended.outcome, ended.note
        );
        self.ledger.finish(&finished);
        self.history.push(finished);
        if !matches!(&action, FlowAction::If(_) | FlowAction::RepeatUntil(_)) || halt.is_some() {
            self.reports.push(StepReport {
                path,
                kind: kind.to_owned(),
                text,
                outcome: ended.outcome,
                turns: log.turns,
                jev_calls: log.calls,
                actions: log.actions,
                loops: log.loops.into_iter().collect(),
                confidence: log.confidence,
                note: ended.note,
            });
        } else {
            let position = self
                .reports
                .iter()
                .position(|report| report.path.starts_with(&format!("{path}.")))
                .unwrap_or(self.reports.len());
            self.reports.insert(
                position,
                StepReport {
                    path,
                    kind: kind.to_owned(),
                    text,
                    outcome: ended.outcome,
                    turns: log.turns,
                    jev_calls: log.calls,
                    actions: log.actions,
                    loops: log.loops.into_iter().collect(),
                    confidence: log.confidence,
                    note: ended.note,
                },
            );
        }
        match halt {
            Some(Halt::Failed(_)) => Err(Halt::Stop(FlowStopReason::StepFailed)),
            Some(halt) => Err(halt),
            None => Ok(()),
        }
    }

    /// Resets what one step keeps, before step `path` runs.
    fn begin_step(&mut self, path: &str) {
        path.clone_into(&mut self.step);
        self.ledger.begin();
        self.refused.clear();
        self.frontier.clear();
        self.step_location.clone_from(&self.location);
    }

    pub(super) fn enabled(&self, flow_loop: FlowLoop) -> bool {
        !self.disabled.contains(&flow_loop)
    }

    /// Whether the run deliberates on evidence at all, and `flow_loop` —
    /// one of deliberation's loops — is on.
    pub(super) fn deliberates(&self, flow_loop: FlowLoop) -> bool {
        self.deliberation != Deliberation::Off && self.enabled(flow_loop)
    }

    /// Whether the run deliberates at the deep level.
    pub(super) fn deep(&self) -> bool {
        self.deliberation == Deliberation::Deep
    }

    /// Jev evaluations the run may still make.
    pub(super) fn room(&self) -> u32 {
        self.max_calls.saturating_sub(self.metrics.calls)
    }

    pub(super) fn model(&self) -> &str {
        &self.runtime.configuration.model
    }

    /// Asks Jev one request, charging it to the run and the step.
    ///
    /// The request is briefed and masked first, then asked in as many
    /// framings as the run votes with — concurrently, each one charged as an
    /// evaluation — and the answers are averaged. On a web page it also
    /// carries a page-kind question, whose answer briefs the next request.
    pub(super) async fn ask(
        &mut self,
        log: &mut StepLog,
        request: EvaluationRequest,
    ) -> Result<BTreeMap<String, Answer>, Halt> {
        let mut answers = self.ask_batch(log, vec![request]).await?;
        answers
            .pop()
            .ok_or_else(|| Halt::Failed("no Jev evaluation completed".to_owned()))
    }

    /// Asks Jev several independent requests at once: one round trip, not
    /// one per request. Each is briefed, masked, fitted, and voted on
    /// exactly as [`FlowRun::ask`] would, every framing of every request is
    /// in flight together, and the answers come back in request order.
    ///
    /// The first request is the one the caller needs; the rest may be
    /// speculative. When the budget has no room for all of them at full
    /// votes, the batch is cut from the end — never below the first — so the
    /// reply may be shorter than `requests`.
    pub(super) async fn ask_batch(
        &mut self,
        log: &mut StepLog,
        mut requests: Vec<EvaluationRequest>,
    ) -> Result<Vec<BTreeMap<String, Answer>>, Halt> {
        if self.metrics.calls >= self.max_calls {
            return Err(Halt::Stop(FlowStopReason::ModelBudget));
        }
        let room = self.max_calls - self.metrics.calls;
        let votes = if self.enabled(FlowLoop::Vote) {
            self.votes.max(1)
        } else {
            1
        };
        let affordable = usize::try_from(room / votes).unwrap_or(usize::MAX).max(1);
        requests.truncate(affordable);
        let votes = votes.min(room);
        if votes > 1 {
            log.used(FlowLoop::Vote);
        }
        let batched = requests.len();
        let mut asked = Vec::with_capacity(batched);
        for request in requests {
            let request = self.outgoing(log, request);
            let framings = vote::framings(&request, votes);
            let handles = self.spawn(&framings);
            asked.push((request, framings, handles));
        }
        self.rounds = self.rounds.saturating_add(1);
        let asked_at = Instant::now();
        let mut replies = Vec::with_capacity(batched);
        for (request, framings, handles) in asked {
            self.decisions = self.decisions.saturating_add(1);
            let mut answered = Vec::new();
            let mut failure = None;
            for (framing, handle) in framings.into_iter().zip(handles) {
                match handle.await {
                    Ok(Ok(evaluation)) => {
                        merge_metrics(&mut self.metrics, &evaluation);
                        log.calls = log.calls.saturating_add(1);
                        answered.push((framing, evaluation.response.answers));
                    }
                    Ok(Err(error)) => {
                        failure.get_or_insert(error);
                    }
                    Err(_) => {}
                }
            }
            let answers = match (answered.is_empty(), failure) {
                (true, Some(failure)) => return Err(Halt::Error(provider_error(&failure))),
                (true, None) => {
                    return Err(Halt::Failed("no Jev evaluation completed".to_owned()));
                }
                _ => {
                    let ballots = vote::ballots(&answered);
                    let merged = vote::tally(&ballots);
                    self.ballots.extend(ballots);
                    merged
                }
            };
            // The decision's wall time: its framings run at once, and the
            // batch's requests with them, so this is the slowest framing so
            // far plus the merge — what the step waited for this answer.
            self.runtime.journal.record("decision", || {
                json!({
                    "step": self.step,
                    "questions": request.questions.keys().collect::<Vec<_>>(),
                    "framings": votes,
                    "answered": answered.len(),
                    "batched": batched,
                    "request_bytes": serde_json::to_vec(&request).map_or(0, |bytes| bytes.len()),
                    "wall_ms": millis(asked_at.elapsed()),
                })
            });
            if self.tracing {
                self.trace.push(JevExchange {
                    step: self.step.clone(),
                    state: request.state.clone(),
                    questions: serde_json::to_value(&request.questions).unwrap_or_default(),
                    answers: serde_json::to_value(&answers).unwrap_or_default(),
                });
            }
            if let Some((kind, _)) = ask::chosen(&answers, PAGE_KIND) {
                self.page = Some(kind);
            }
            replies.push(answers);
        }
        Ok(replies)
    }

    /// `request` as it leaves for Jev: with the page-kind question on a web
    /// page, briefed, masked, and fitted to size.
    fn outgoing(&self, log: &mut StepLog, mut request: EvaluationRequest) -> EvaluationRequest {
        if self.enabled(FlowLoop::PageKind) && self.app == crate::workspace::BROWSER {
            log.used(FlowLoop::PageKind);
            request
                .questions
                .insert(PAGE_KIND.to_owned(), ask::page_kind());
        }
        self.brief_into(&mut request);
        self.mask(&mut request);
        fit(&mut request, MAX_REQUEST_BYTES);
        request
    }

    /// Sends every framing to Jev at once.
    fn spawn(
        &self,
        framings: &[vote::Framing],
    ) -> Vec<
        tokio::task::JoinHandle<
            Result<
                tinyinference_decisions::EvaluationResult,
                tinyinference_decisions::EvaluationFailure,
            >,
        >,
    > {
        framings
            .iter()
            .map(|framing| {
                let runtime = self.runtime.clone();
                let step = self.step.clone();
                let request = framing.request.clone();
                tokio::spawn(async move { runtime.evaluate(Some(&step), &request).await })
            })
            .collect()
    }

    /// Adds the run's brief — the goal, whom it is for, the plan with this
    /// step marked, what has been chosen so far, and the kind of page showing
    /// — to the questions that choose: which element, option, move, field,
    /// or record, and whether an element is the right one.
    ///
    /// A yes/no judgement of the screen (is the step done, does a condition
    /// hold, is something in the way) is left without it. Measured on a live
    /// results page, the brief pulled Jev's "is the search done?" from 0.75
    /// down to 0.39: it judged the step against the whole task.
    fn brief_into(&self, request: &mut EvaluationRequest) {
        let Some(brief) = self.brief() else {
            return;
        };
        for (id, question) in &mut request.questions {
            let instructions = match question {
                Question::Choice(choice) if id != PAGE_KIND => &mut choice.instructions,
                Question::Noul(noul)
                    if BRIEFED_NOULS.contains(&id.as_str())
                        || id.starts_with("is_")
                        || id.starts_with("only_near_") =>
                {
                    &mut noul.instructions
                }
                _ => continue,
            };
            if let Value::Object(fields) = instructions {
                fields.insert("brief".to_owned(), brief.clone());
            }
        }
    }

    /// The brief as Jev reads it, or `None` when there is nothing to say.
    fn brief(&self) -> Option<Value> {
        let current = self
            .step
            .split('.')
            .next()
            .and_then(|top| top.parse::<usize>().ok())
            .unwrap_or(0);
        let plan = self
            .outline
            .iter()
            .enumerate()
            .map(|(index, step)| {
                let mark = match (index + 1).cmp(&current) {
                    std::cmp::Ordering::Less => "done",
                    std::cmp::Ordering::Equal => "now",
                    std::cmp::Ordering::Greater => "next",
                };
                clip(&format!("{}. [{mark}] {step}", index + 1), MAX_PLAN_LINE)
            })
            .collect::<Vec<_>>();
        let mut brief = serde_json::Map::new();
        if !self.brief.goal.is_empty() {
            brief.insert("goal".to_owned(), json!(clip(&self.brief.goal, MAX_GOAL)));
        }
        if !self.brief.details.is_empty() {
            brief.insert("for".to_owned(), json!(self.brief.details));
        }
        if !self.brief.secrets.is_empty() {
            brief.insert(
                "secrets".to_owned(),
                json!({
                    "note": "Held locally and typed by the module; you only ever see them as these names.",
                    "names": self.brief.secrets.iter().map(|name| format!("${{{name}}}")).collect::<Vec<_>>(),
                }),
            );
        }
        if !self.brief.rules.is_empty() {
            brief.insert("rules".to_owned(), json!(self.brief.rules));
        }
        if plan.len() > 1 {
            brief.insert("plan".to_owned(), json!(plan));
        }
        if !self.so_far.is_empty() {
            brief.insert(
                "so_far".to_owned(),
                json!(
                    self.so_far
                        .iter()
                        .map(|note| clip(note, MAX_SO_FAR_NOTE))
                        .collect::<Vec<_>>()
                ),
            );
        }
        if let Some(page) = &self.page {
            brief.insert("page".to_owned(), json!(page));
        }
        (!brief.is_empty()).then_some(Value::Object(brief))
    }

    /// Masks every secret out of a request, wherever it appears.
    fn mask(&self, request: &mut EvaluationRequest) {
        if self.secrets.secret_names().is_empty() {
            return;
        }
        mask_value(&mut request.state, &self.secrets);
        for question in request.questions.values_mut() {
            let mut value = serde_json::to_value(&*question).unwrap_or_default();
            mask_value(&mut value, &self.secrets);
            if let Ok(masked) = serde_json::from_value(value) {
                *question = masked;
            }
        }
    }

    /// Notes something the run chose or entered, for the brief's `so_far`.
    pub(super) fn remember_choice(&mut self, note: &str) {
        self.so_far.push(self.secrets.mask(note));
        if self.so_far.len() > MAX_SO_FAR {
            self.so_far.remove(0);
        }
    }

    /// Reads the application's current surface.
    ///
    /// An application can be running with nothing readable on screen — a
    /// document app showing only its open panel, or one still starting. That
    /// is reported to Jev as a blank screen with a note, so a keyboard move can
    /// still make progress; only [`MAX_BLIND_LOOKS`] such looks in a row fail
    /// the step.
    pub(super) async fn look(&mut self) -> Result<Screen, Halt> {
        let started = Instant::now();
        let observed =
            observe_async(self.backend.clone(), self.app.clone(), None, Depth::Full).await;
        self.runtime.journal.record("observe", || {
            json!({
                "step": self.step,
                "part": "screen",
                "wall_ms": millis(started.elapsed()),
                "ok": observed.is_ok(),
                "candidates": observed.as_ref().map_or(0, |screen| screen.candidates.len()),
                "unexplored": observed.as_ref().map_or(0, |screen| screen.unexplored.len()),
            })
        });
        match observed {
            Ok(screen) => {
                self.blind_looks = 0;
                Ok(screen)
            }
            Err(error) => {
                let reason = error
                    .error
                    .as_ref()
                    .map_or_else(|| "unknown error".to_owned(), |error| error.message.clone());
                self.blind_looks = self.blind_looks.saturating_add(1);
                if self.blind_looks >= MAX_BLIND_LOOKS {
                    return Err(Halt::Failed(format!(
                        "the screen could not be read: {reason}"
                    )));
                }
                Ok(Screen {
                    app: self.app.clone(),
                    window: None,
                    surface: "none".to_owned(),
                    candidates: Vec::new(),
                    context: vec![format!(
                        "No window of the application can be read right now ({reason}). A keyboard shortcut may still work."
                    )],
                    unexplored: Vec::new(),
                    text_nodes: Vec::new(),
                })
            }
        }
    }

    /// Reads the subtrees the engine cut short and adds what they hold.
    ///
    /// Called only when a step did not find what it needs in the budgeted
    /// view — a note editor below a long folder list — so the common case
    /// still reads one bounded snapshot.
    pub(super) async fn explore(&self, screen: &mut Screen) {
        for root in std::mem::take(&mut screen.unexplored)
            .into_iter()
            .take(MAX_EXPLORED)
        {
            let started = Instant::now();
            let part = observe_async(
                self.backend.clone(),
                self.app.clone(),
                Some(root),
                Depth::Full,
            )
            .await;
            self.runtime.journal.record("observe", || {
                json!({
                    "step": self.step,
                    "part": "subtree",
                    "wall_ms": millis(started.elapsed()),
                    "ok": part.is_ok(),
                    "candidates": part.as_ref().map_or(0, |part| part.candidates.len()),
                })
            });
            if let Ok(part) = part {
                screen.candidates.extend(part.candidates);
                screen.context.extend(part.context);
                screen.text_nodes.extend(part.text_nodes);
            }
        }
    }

    /// Runs one desktop action, charging it to the budget and the step log.
    pub(super) async fn act<F>(
        &mut self,
        log: &mut StepLog,
        action: &str,
        target: Option<&Candidate>,
        call: F,
    ) -> Result<DesktopResponse, Halt>
    where
        F: FnOnce(B) -> DesktopResponse + Send + 'static,
    {
        if self.actions >= self.max_actions {
            return Err(Halt::Stop(FlowStopReason::ActionBudget));
        }
        self.actions = self.actions.saturating_add(1);
        let started = Instant::now();
        let reply = self.backend_call(call).await;
        let acted_ms = millis(started.elapsed());
        if let Some(url) = reply
            .data
            .as_ref()
            .and_then(|data| data.get("url"))
            .and_then(Value::as_str)
            .filter(|url| !url.is_empty())
        {
            self.location = Some(url.to_owned());
        }
        let note = match (&reply.error, &reply.data) {
            (Some(error), _) => error.code.clone(),
            (None, Some(data)) => data
                .get("path")
                .and_then(serde_json::Value::as_str)
                .map(|path| {
                    let verified = data
                        .get("verified")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(true);
                    format!("via {path}{}", if verified { "" } else { ", unverified" })
                })
                .unwrap_or_default(),
            (None, None) => String::new(),
        };
        log.actions.push(FlowActionRecord {
            action: action.to_owned(),
            target: target.map(target_payload),
            ok: reply.ok,
            note,
        });
        let settle_started = Instant::now();
        if reply.ok {
            // Let the surface finish reacting, so the next look sees what the
            // action did rather than the moment before it took effect.
            self.backend_call(|backend| {
                backend.settle();
                DesktopResponse::ok("settle", serde_json::json!({}))
            })
            .await;
        }
        self.runtime.journal.record("action", || {
            let record = log.actions.last();
            json!({
                "step": self.step,
                "action": action,
                "target": record.and_then(|record| record.target.as_ref()),
                "ok": reply.ok,
                "note": record.map(|record| record.note.as_str()),
                "wall_ms": acted_ms,
                "settle_ms": if reply.ok { millis(settle_started.elapsed()) } else { 0 },
            })
        });
        Ok(reply)
    }

    async fn backend_call<F>(&self, call: F) -> DesktopResponse
    where
        F: FnOnce(B) -> DesktopResponse + Send + 'static,
    {
        blocking(self.backend.clone(), call).await
    }

    fn finish(self, stop: FlowStopReason) -> DesktopResponse {
        self.runtime.journal.record("end", || {
            json!({
                "stop": stop,
                "wall_ms": millis(self.started.elapsed()),
                "actions": self.actions,
                "metrics": self.metrics,
                "learned": self.learned.len(),
            })
        });
        response(
            "run-flow",
            &FlowRunResult {
                stop,
                steps: self.reports,
                vars: self.vars,
                pending: self.pending,
                learned: self.learned,
                actions: self.actions,
                metrics: self.metrics,
                trace: self.trace,
            },
        )
    }
}

/// The id of the page-kind question a request on a web page carries.
const PAGE_KIND: &str = "page_kind";

/// The yes/no questions that are about choosing, not judging the screen:
/// whether an element is the right one for a purpose.
const BRIEFED_NOULS: &[&str] = &["confirm"];

/// `text` cut to `limit` characters, marked with `…` when it was cut.
fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut clipped = text.chars().take(limit).collect::<String>();
    clipped.push('…');
    clipped
}

/// Shrinks `request` until its JSON is at most `limit` bytes: first the
/// brief is kept on the first briefed question only, then the longest lists
/// of screen text and elements in the shared state lose their last entries.
/// What remains is the part of the screen read first.
pub(super) fn fit(request: &mut EvaluationRequest, limit: usize) {
    let size =
        |request: &EvaluationRequest| serde_json::to_vec(request).map_or(0, |json| json.len());
    if size(request) <= limit {
        return;
    }
    let mut kept = false;
    for question in request.questions.values_mut() {
        let instructions = match question {
            Question::Choice(choice) => &mut choice.instructions,
            Question::Noul(noul) => &mut noul.instructions,
            Question::Score(score) => &mut score.instructions,
        };
        if let Value::Object(fields) = instructions
            && fields.contains_key("brief")
        {
            if kept {
                fields.remove("brief");
            }
            kept = true;
        }
    }
    while size(request) > limit {
        let Some(longest) = longest_list(&mut request.state) else {
            return;
        };
        let cut = (longest.len() / 4).max(1);
        longest.truncate(longest.len() - cut);
    }
}

/// The longest non-empty array anywhere in `value`.
fn longest_list(value: &mut Value) -> Option<&mut Vec<Value>> {
    let mut best: Option<&mut Vec<Value>> = None;
    let candidates: Vec<&mut Vec<Value>> = match value {
        Value::Array(items) => {
            if items
                .iter()
                .all(|item| !item.is_array() && !item.is_object())
            {
                return (!items.is_empty()).then_some(items);
            }
            items.iter_mut().filter_map(longest_list).collect()
        }
        Value::Object(fields) => fields.values_mut().filter_map(longest_list).collect(),
        _ => Vec::new(),
    };
    for candidate in candidates {
        if best
            .as_ref()
            .is_none_or(|best| candidate.len() > best.len())
        {
            best = Some(candidate);
        }
    }
    best
}

/// Every string inside `value` with the secrets masked.
fn mask_value(value: &mut Value, secrets: &Facts) {
    match value {
        Value::String(text) => *text = secrets.mask(text),
        Value::Array(items) => items.iter_mut().for_each(|item| mask_value(item, secrets)),
        Value::Object(fields) => fields
            .values_mut()
            .for_each(|field| mask_value(field, secrets)),
        _ => {}
    }
}

/// A step's wire kind and its text with variables substituted.
///
/// This text is what a step report shows and what `recent_actions` carries
/// into every later Jev question, so it is built with [`substitute_safe`]:
/// even a step kind that may substitute a fact operationally (`open`,
/// `browse`) never repeats that value here.
fn describe_step(
    action: &FlowAction,
    vars: &BTreeMap<String, String>,
    facts: &BTreeSet<String>,
) -> (&'static str, String) {
    let (kind, text) = match action {
        FlowAction::Open(app) => ("open", app.clone()),
        FlowAction::Browse(url) => ("browse", url.clone()),
        FlowAction::Do(intent) => ("do", intent.clone()),
        FlowAction::Enter(slots) => (
            "enter",
            slots
                .0
                .iter()
                .map(|slot| slot.slot.clone())
                .collect::<Vec<_>>()
                .join(", "),
        ),
        FlowAction::Choose(choose) => ("choose", format!("{} in {}", choose.option, choose.what)),
        FlowAction::Read(read) => ("read", format!("{} into {}", read.what, read.into)),
        FlowAction::Pick(pick) => ("pick", format!("{} by {}", pick.from, pick.by)),
        FlowAction::Extract(read) => ("extract", format!("{} into {}", read.what, read.into)),
        FlowAction::Verify(condition) => ("verify", condition.clone()),
        FlowAction::WaitFor(condition) => ("wait_for", condition.clone()),
        FlowAction::StopBefore(action) => ("stop_before", action.clone()),
        FlowAction::RepeatUntil(repeat) => ("repeat_until", repeat.condition.clone()),
        FlowAction::If(branch) => ("if", branch.condition.clone()),
    };
    (kind, substitute_safe(&text, vars, facts))
}
