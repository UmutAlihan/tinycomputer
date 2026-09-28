//! Bounded, single-call desktop task execution.
//!
//! The loop observes, asks Jev for one decision (`decision`), and performs it
//! and checks what changed (`step`), until the goal is visibly done or a
//! budget runs out.

mod decision;
mod step;

use std::time::{Duration, Instant};

use tinycomputer_bus::{
    DesktopError, DesktopResponse, JevDecision, JevMetrics, JevObservation, JevStopReason,
    JevTurn, RunGoalRequest, VisiblePredicate,
};

use super::backend::{AgentBackend, observe_async};
use super::goal::within_scope;
use super::reply::{action_failed_response, merge_metrics, run_response_observed};
use super::resolve::resolve_on_screen;
use super::runtime::JevRuntime;
use super::screen::Screen;
use super::verify::{satisfied, verify};

enum DecisionFlow {
    Execute,
    Repeat,
    Stop(Box<DesktopResponse>),
}

const APP_READY_WAIT: Duration = Duration::from_secs(3);

fn app_may_still_be_starting(reply: &DesktopResponse) -> bool {
    reply
        .error
        .as_ref()
        .is_some_and(|error| matches!(error.code.as_str(), "APP_NOT_FOUND" | "WINDOW_NOT_FOUND"))
}

struct GoalLoop<B> {
    backend: B,
    runtime: JevRuntime,
    request: RunGoalRequest,
    started: Instant,
    max_elapsed: Duration,
    max_steps: u32,
    max_calls: u32,
    texts: std::vec::IntoIter<String>,
    next_text: Option<String>,
    root: Option<String>,
    turns: Vec<JevTurn>,
    metrics: JevMetrics,
    history: Vec<String>,
    unchanged: u32,
    last_observation: Option<JevObservation>,
    verification_retries: u8,
    low_confidence_retries: u8,
}

pub(super) async fn run_goal_fresh<B: AgentBackend>(
    backend: B,
    runtime: JevRuntime,
    request: RunGoalRequest,
    history: Vec<String>,
    unchanged: u32,
) -> DesktopResponse {
    if !valid_task_scope(&request) {
        return DesktopResponse::err(
            "run-goal",
            DesktopError::new(
                "INVALID_TASK_SCOPE",
                "desktop task has a blank condition or lacks continuous-execution scope",
            ),
        );
    }
    let mut texts = request.text.clone().into_iter();
    let mut task = GoalLoop {
        backend,
        runtime,
        started: Instant::now(),
        max_elapsed: Duration::from_millis(request.max_elapsed_ms.clamp(1, 300_000)),
        max_steps: request.max_steps.clamp(1, 40),
        max_calls: request.max_model_calls.clamp(1, 80),
        next_text: texts.next(),
        texts,
        root: request.root.clone(),
        request,
        turns: Vec::new(),
        metrics: JevMetrics::default(),
        history,
        unchanged,
        last_observation: None,
        verification_retries: 0,
        low_confidence_retries: 0,
    };
    loop {
        if let Some(reply) = task.one_turn().await {
            return reply;
        }
    }
}

fn valid_task_scope(request: &RunGoalRequest) -> bool {
    if request
        .allowed_targets
        .iter()
        .any(|label| label.trim().is_empty())
        || request
            .text_slots
            .keys()
            .any(|label| label.trim().is_empty())
        || request.success.iter().any(|predicate| match predicate {
            VisiblePredicate::NamePresent { name } | VisiblePredicate::ValueEquals { name, .. } => {
                name.trim().is_empty()
            }
            VisiblePredicate::NameContains { fragment, within } => {
                fragment.trim().is_empty() || within.trim().is_empty()
            }
            VisiblePredicate::ValueContains { name, value } => {
                name.trim().is_empty() || value.is_empty()
            }
            VisiblePredicate::StateContains { name, state } => {
                name.trim().is_empty() || state.trim().is_empty()
            }
        })
    {
        return false;
    }
    request.require_confirmations
        || (!request.allowed_operations.is_empty()
            && !request.allowed_targets.is_empty()
            && !request.success.is_empty())
}

impl<B: AgentBackend> GoalLoop<B> {
    fn stop(&self, reason: JevStopReason, pending: Option<JevDecision>) -> DesktopResponse {
        run_response_observed(
            reason,
            self.turns.clone(),
            pending,
            self.metrics.clone(),
            self.last_observation.clone(),
        )
    }

    fn budget(&self) -> Option<JevStopReason> {
        if self.started.elapsed() >= self.max_elapsed {
            Some(JevStopReason::TimeBudget)
        } else if u32::try_from(self.turns.len()).unwrap_or(u32::MAX) >= self.max_steps {
            Some(JevStopReason::ActionBudget)
        } else if self.metrics.calls >= self.max_calls {
            Some(JevStopReason::ModelBudget)
        } else {
            None
        }
    }

    async fn observe(&self) -> Result<Screen, Box<DesktopResponse>> {
        let ready_until = Instant::now() + APP_READY_WAIT;
        loop {
            let remaining = self.max_elapsed.saturating_sub(self.started.elapsed());
            if remaining.is_zero() {
                return Err(Box::new(self.stop(JevStopReason::TimeBudget, None)));
            }
            match tokio::time::timeout(
                remaining,
                observe_async(
                    self.backend.clone(),
                    self.request.app.clone(),
                    self.request.window_id.clone(),
                    self.root.clone(),
                ),
            )
            .await
            {
                Ok(Ok(screen)) => return Ok(screen),
                Ok(Err(error))
                    if app_may_still_be_starting(&error) && Instant::now() < ready_until =>
                {
                    tokio::time::sleep(Duration::from_millis(200).min(remaining)).await;
                }
                Ok(Err(error)) => return Err(error),
                Err(_) => return Err(Box::new(self.stop(JevStopReason::TimeBudget, None))),
            }
        }
    }

    async fn one_turn(&mut self) -> Option<DesktopResponse> {
        if let Some(stop) = self.budget() {
            return Some(self.stop(stop, None));
        }
        let before = match self.observe().await {
            Ok(screen) => screen,
            Err(reply) => return Some(*reply),
        };
        if !within_scope(&self.request, &before) {
            return Some(self.stop(JevStopReason::ScopeChanged, None));
        }
        if !self.request.success.is_empty() {
            let evidence = verify(&before, &self.request.success);
            let done = satisfied(&evidence);
            self.last_observation = Some(evidence);
            if done {
                return Some(self.stop(JevStopReason::Done, None));
            }
        }
        let decision = match self.decide(&before).await {
            Ok(decision) => decision,
            Err(reply) => return Some(*reply),
        };
        match self.handle_decision(&before, &decision) {
            DecisionFlow::Stop(reply) => Some(*reply),
            DecisionFlow::Repeat => None,
            DecisionFlow::Execute => self.execute_step(&before, decision).await,
        }
    }

    async fn decide(&mut self, before: &Screen) -> Result<JevDecision, Box<DesktopResponse>> {
        let text = self
            .next_text
            .as_deref()
            .or_else(|| (!self.request.text_slots.is_empty()).then_some(""));
        let outcome = tokio::time::timeout(
            self.max_elapsed.saturating_sub(self.started.elapsed()),
            resolve_on_screen(
                &self.backend,
                &self.runtime,
                &self.request.goal,
                before,
                text,
                self.request.include_values,
                false,
                &self.history,
                self.metrics.calls.saturating_add(1) < self.max_calls,
                Some(&self.request),
            ),
        )
        .await;
        let outcome = match outcome {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(error)) => return Err(error),
            Err(_) => return Err(Box::new(self.stop(JevStopReason::TimeBudget, None))),
        };
        for evaluation in &outcome.evaluations {
            merge_metrics(&mut self.metrics, evaluation);
        }
        if let Some(failure) = outcome.action_failure {
            return Err(Box::new(action_failed_response(
                self.turns.clone(),
                outcome.decision,
                self.metrics.clone(),
                &failure,
            )));
        }
        Ok(outcome.decision)
    }
}
