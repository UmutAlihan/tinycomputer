//! Bounded, single-call desktop task execution.

use std::time::{Duration, Instant};

use tinydesktop_bus::{
    DesktopError, DesktopResponse, JevDecision, JevMetrics, JevObservation, JevOperation,
    JevStopReason, JevTurn, RunGoalRequest, VisiblePredicate,
};

use super::{
    AgentBackend, Candidate, JevRuntime, PendingRun, Screen, action_failed_response,
    execute_operation, failed_turn, fingerprint, merge_metrics, mutates, observe_async,
    prepared_text, queue_confirmation, record_turn, resolve_on_screen, run_response_observed,
    same_target, satisfied, selected_target, stop_reason, target_allowed, verify, within_scope,
};

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

    fn handle_decision(&mut self, before: &Screen, decision: &JevDecision) -> DecisionFlow {
        let stop = stop_reason(decision.decision);
        if stop == Some(JevStopReason::ConfirmationRequired) && self.request.require_confirmations {
            return DecisionFlow::Stop(Box::new(self.confirmation(before, decision)));
        }
        if stop == Some(JevStopReason::LowConfidence)
            && decision.target.is_some()
            && self.low_confidence_retries < 1
        {
            self.low_confidence_retries += 1;
            return DecisionFlow::Repeat;
        }
        if stop == Some(JevStopReason::Done) && !self.request.success.is_empty() {
            if self.verification_retries < 1 {
                self.verification_retries += 1;
                self.history.push(
                    "DONE was rejected: the required visible success conditions are not yet met. Choose a next action from the current screen."
                        .to_owned(),
                );
                return DecisionFlow::Repeat;
            }
            return DecisionFlow::Stop(Box::new(
                self.stop(JevStopReason::VerificationFailed, Some(decision.clone())),
            ));
        }
        if let Some(stop) = stop
            && stop != JevStopReason::ConfirmationRequired
        {
            return DecisionFlow::Stop(Box::new(self.stop(stop, Some(decision.clone()))));
        }
        self.low_confidence_retries = 0;
        DecisionFlow::Execute
    }

    fn confirmation(&self, before: &Screen, decision: &JevDecision) -> DesktopResponse {
        let Some(target) = selected_target(before, decision) else {
            return self.stop(JevStopReason::StaleTarget, Some(decision.clone()));
        };
        queue_confirmation(
            &self.runtime,
            PendingRun {
                created: Instant::now(),
                started: self.started,
                request: RunGoalRequest {
                    text: self
                        .next_text
                        .clone()
                        .into_iter()
                        .chain(self.texts.clone())
                        .collect(),
                    root: self.root.clone(),
                    max_steps: self
                        .max_steps
                        .saturating_sub(u32::try_from(self.turns.len()).unwrap_or(u32::MAX)),
                    max_model_calls: self.max_calls.saturating_sub(self.metrics.calls),
                    ..self.request.clone()
                },
                decision: decision.clone(),
                screen: before.clone(),
                target,
                turns: self.turns.clone(),
                history: self.history.clone(),
                unchanged: self.unchanged,
                metrics: self.metrics.clone(),
            },
        )
    }

    async fn execute_step(
        &mut self,
        before: &Screen,
        decision: JevDecision,
    ) -> Option<DesktopResponse> {
        let fresh = match self.observe().await {
            Ok(screen) => screen,
            Err(reply) => return Some(*reply),
        };
        let target = match self.current_target(before, &fresh, &decision) {
            Ok(target) => target,
            Err(stop) => return Some(self.stop(stop, Some(decision))),
        };
        let text = if decision.operation == JevOperation::TypeText {
            target
                .as_ref()
                .and_then(|candidate| prepared_text(&self.request, candidate))
                .or_else(|| self.next_text.clone())
        } else {
            None
        };
        if decision.operation == JevOperation::TypeText && text.is_none() {
            return Some(self.stop(JevStopReason::NeedsText, Some(decision)));
        }
        let Ok(reply) = tokio::time::timeout(
            self.max_elapsed.saturating_sub(self.started.elapsed()),
            execute_operation(self.backend.clone(), decision.operation, target, text),
        )
        .await
        else {
            self.turns.push(failed_turn(&self.turns, &decision));
            return Some(self.stop(JevStopReason::ActionUncertain, Some(decision)));
        };
        if !reply.ok {
            return Some(action_failed_response(
                self.turns.clone(),
                decision,
                self.metrics.clone(),
                &reply,
            ));
        }
        let decision = JevDecision {
            executed: true,
            ..decision
        };
        if decision.operation == JevOperation::TypeText && self.request.text_slots.is_empty() {
            self.next_text = self.texts.next();
        }
        if decision.operation == JevOperation::Drill {
            self.root = decision.target.as_ref().map(|target| target.ref_id.clone());
        } else if decision.operation == JevOperation::Widen {
            self.root = None;
        }
        let delivered_unverified = reply
            .data
            .as_ref()
            .and_then(|data| data.get("disposition"))
            .and_then(|disposition| disposition.get("delivery"))
            .and_then(serde_json::Value::as_str)
            == Some("delivered_unverified");
        self.after_action(before, decision, delivered_unverified)
            .await
    }

    fn current_target(
        &self,
        before: &Screen,
        fresh: &Screen,
        decision: &JevDecision,
    ) -> Result<Option<Candidate>, JevStopReason> {
        if !within_scope(&self.request, fresh) {
            return Err(JevStopReason::ScopeChanged);
        }
        if !self.request.allowed_operations.is_empty()
            && mutates(decision.operation)
            && !self
                .request
                .allowed_operations
                .contains(&decision.operation)
        {
            return Err(JevStopReason::ScopeChanged);
        }
        let Some(selected) = selected_target(before, decision) else {
            return if decision.target.is_some() {
                Err(JevStopReason::StaleTarget)
            } else {
                Ok(None)
            };
        };
        let mut matching = fresh.candidates.iter().filter(|candidate| {
            same_target(before, fresh, &selected, candidate, decision.operation)
        });
        let current = matching.next().cloned().ok_or(JevStopReason::StaleTarget)?;
        if matching.next().is_some() {
            return Err(JevStopReason::StaleTarget);
        }
        if !target_allowed(&self.request, &current)
            || self
                .request
                .allowed_targets
                .iter()
                .filter(|label| super::exact_label(&current, label))
                .any(|label| {
                    fresh
                        .candidates
                        .iter()
                        .filter(|candidate| super::exact_label(candidate, label))
                        .count()
                        != 1
                })
        {
            return Err(JevStopReason::ScopeChanged);
        }
        Ok(Some(current))
    }

    async fn after_action(
        &mut self,
        before: &Screen,
        decision: JevDecision,
        delivered_unverified: bool,
    ) -> Option<DesktopResponse> {
        let after = self.observe().await;
        let Ok(mut after) = after else {
            record_turn(&mut self.turns, &mut self.history, &decision, false);
            return Some(self.stop(JevStopReason::ActionUncertain, Some(decision)));
        };
        if !within_scope(&self.request, &after) {
            record_turn(&mut self.turns, &mut self.history, &decision, false);
            return Some(self.stop(JevStopReason::ScopeChanged, Some(decision)));
        }
        if !self.request.success.is_empty() {
            let evidence = verify(&after, &self.request.success);
            let done = satisfied(&evidence);
            self.last_observation = Some(evidence);
            if done {
                record_turn(&mut self.turns, &mut self.history, &decision, true);
                return Some(self.stop(JevStopReason::Done, None));
            }
            if delivered_unverified {
                let settle = Instant::now() + Duration::from_secs(2);
                while Instant::now() < settle && self.started.elapsed() < self.max_elapsed {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    let Ok(fresh) = self.observe().await else {
                        continue;
                    };
                    if !within_scope(&self.request, &fresh) {
                        record_turn(&mut self.turns, &mut self.history, &decision, false);
                        return Some(self.stop(JevStopReason::ScopeChanged, Some(decision)));
                    }
                    let evidence = verify(&fresh, &self.request.success);
                    let done = satisfied(&evidence);
                    self.last_observation = Some(evidence);
                    after = fresh;
                    if done {
                        record_turn(&mut self.turns, &mut self.history, &decision, true);
                        return Some(self.stop(JevStopReason::Done, None));
                    }
                }
            }
        }
        if delivered_unverified && decision.destructive >= super::policy::DESTRUCTIVE {
            record_turn(&mut self.turns, &mut self.history, &decision, false);
            return Some(self.stop(JevStopReason::ActionUncertain, Some(decision)));
        }
        let changed = fingerprint(&after) != fingerprint(before)
            || matches!(
                decision.operation,
                JevOperation::Drill | JevOperation::Widen
            );
        self.unchanged = if changed {
            0
        } else {
            self.unchanged.saturating_add(1)
        };
        record_turn(&mut self.turns, &mut self.history, &decision, changed);
        (self.unchanged >= 3).then(|| self.stop(JevStopReason::Stalled, None))
    }
}
