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
//!
//! See `docs/specs/jev-intent-flows.md` for the design and its rationale.

mod act;
mod ask;
mod enter;
mod ground;
mod memory;
mod steps;
mod validate;

#[cfg(test)]
mod test;

use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
};

use serde_json::json;
use tinydesktop_bus::{
    DesktopError, DesktopResponse, FLOW_GUIDE, Flow, FlowAction, FlowActionRecord, FlowLoop,
    FlowRunResult, FlowStep, FlowStopReason, GroundingHint, JevExchange, JevMetrics, JevTarget,
    RunFlowRequest, StepOutcome, StepReport, ValidateFlowRequest,
};
use tinyjevclient::{Answer, EvaluationRequest};

use super::{
    JevRuntime,
    backend::{AgentBackend, blocking, observe_async},
    merge_metrics, provider_error, response,
    screen::{Candidate, Depth, Screen},
    target_payload,
};
use crate::Desktop;
use validate::{step_path, substitute};

/// Upper bound on [`RunFlowRequest::max_actions`].
const MAX_ACTIONS: u32 = 120;
/// Upper bound on [`RunFlowRequest::max_model_calls`].
const MAX_CALLS: u32 = 300;
/// Consecutive unreadable observations that fail a step.
const MAX_BLIND_LOOKS: u32 = 3;

/// Runs `request` against the live desktop.
pub(crate) async fn run_flow(
    desktop: Desktop,
    runtime: JevRuntime,
    request: RunFlowRequest,
) -> DesktopResponse {
    run_flow_with(desktop, &runtime, request).await
}

/// Checks a flow without touching the desktop or Jev.
pub(crate) fn validate_flow(request: &ValidateFlowRequest) -> DesktopResponse {
    let (_, validation) = validate::validate(&request.flow, &BTreeSet::new());
    response("validate-flow", &validation)
}

/// The flow authoring guide, as prompt text.
pub(crate) fn flow_guide() -> DesktopResponse {
    DesktopResponse::ok(
        "flow-guide",
        json!({"guide": FLOW_GUIDE, "step_kinds": tinydesktop_bus::STEP_KINDS}),
    )
}

pub(super) async fn run_flow_with<B: AgentBackend + Sync>(
    backend: B,
    runtime: &JevRuntime,
    request: RunFlowRequest,
) -> DesktopResponse {
    let known = request.vars.keys().cloned().collect::<BTreeSet<_>>();
    let validation = validate::check(&request.flow, &known);
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
    step: String,
}

impl<'r, B: AgentBackend + Sync> FlowRun<'r, B> {
    fn new(backend: B, runtime: &'r JevRuntime, request: &RunFlowRequest) -> Self {
        let mut vars = request.flow.vars.clone();
        vars.extend(request.vars.clone());
        Self {
            backend,
            runtime,
            app: request.flow.app.clone(),
            vars,
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
            step: String::new(),
        }
    }

    async fn start(&mut self, flow: &Flow) -> Result<(), Halt> {
        let app = self.app.clone();
        let _activated = self.backend_call(move |backend| backend.launch(&app)).await;
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
        let (kind, text) = describe_step(&action, &self.vars);
        let mut log = StepLog::default();
        self.step.clone_from(&path);
        let result = steps::run(self, &mut log, &action, &text, &path).await;
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
        self.history.push(format!(
            "step {path} ({kind} {text:?}): {:?}, {}",
            ended.outcome, ended.note
        ));
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

    pub(super) fn enabled(&self, flow_loop: FlowLoop) -> bool {
        !self.disabled.contains(&flow_loop)
    }

    pub(super) fn model(&self) -> &str {
        &self.runtime.configuration.model
    }

    /// Asks Jev one request, charging it to the run and the step.
    pub(super) async fn ask(
        &mut self,
        log: &mut StepLog,
        request: EvaluationRequest,
    ) -> Result<BTreeMap<String, Answer>, Halt> {
        if self.metrics.calls >= self.max_calls {
            return Err(Halt::Stop(FlowStopReason::ModelBudget));
        }
        let evaluation = self
            .runtime
            .client
            .evaluate(&request)
            .await
            .map_err(|error| Halt::Error(provider_error(&error)))?;
        merge_metrics(&mut self.metrics, &evaluation);
        log.calls = log.calls.saturating_add(1);
        if self.tracing {
            self.trace.push(JevExchange {
                step: self.step.clone(),
                state: request.state.clone(),
                questions: serde_json::to_value(&request.questions).unwrap_or_default(),
                answers: serde_json::to_value(&evaluation.response.answers).unwrap_or_default(),
            });
        }
        Ok(evaluation.response.answers)
    }

    /// Reads the application's current surface.
    ///
    /// An application can be running with nothing readable on screen — a
    /// document app showing only its open panel, or one still starting. That
    /// is reported to Jev as a blank screen with a note, so a keyboard move can
    /// still make progress; only [`MAX_BLIND_LOOKS`] such looks in a row fail
    /// the step.
    pub(super) async fn look(&mut self) -> Result<Screen, Halt> {
        match observe_async(self.backend.clone(), self.app.clone(), None, Depth::Full).await {
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
                    root: None,
                    candidates: Vec::new(),
                    context: vec![format!(
                        "No window of the application can be read right now ({reason}). A keyboard shortcut may still work."
                    )],
                    truncated: None,
                })
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
        let reply = self.backend_call(call).await;
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
        Ok(reply)
    }

    async fn backend_call<F>(&self, call: F) -> DesktopResponse
    where
        F: FnOnce(B) -> DesktopResponse + Send + 'static,
    {
        blocking(self.backend.clone(), call).await
    }

    fn finish(self, stop: FlowStopReason) -> DesktopResponse {
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

/// A step's wire kind and its text with variables substituted.
fn describe_step(action: &FlowAction, vars: &BTreeMap<String, String>) -> (&'static str, String) {
    let (kind, text) = match action {
        FlowAction::Open(app) => ("open", app.clone()),
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
        FlowAction::Verify(condition) => ("verify", condition.clone()),
        FlowAction::WaitFor(condition) => ("wait_for", condition.clone()),
        FlowAction::StopBefore(action) => ("stop_before", action.clone()),
        FlowAction::RepeatUntil(repeat) => ("repeat_until", repeat.condition.clone()),
        FlowAction::If(branch) => ("if", branch.condition.clone()),
    };
    (kind, substitute(&text, vars))
}
