//! The bounded observe-decide-act loop behind `RunGoal`.
//!
//! Each turn reuses the previous turn's post-action observation, asks
//! [`resolve_on_screen`] for one gated decision, executes it, and records what
//! changed in terms Jev can read on the next turn. A failed action or a
//! low-confidence turn is fed back and retried up to `max_retries`; an element
//! that fails twice is banned for the rest of the run.

use std::collections::{BTreeMap, BTreeSet};

use tinydesktop_bus::{
    DesktopResponse, JevDecision, JevDecisionKind, JevMetrics, JevOperation, JevRunResult,
    JevStopReason, JevTurn, RunGoalRequest,
};

use super::{
    JevRuntime,
    backend::{AgentBackend, observe_async},
    merge_metrics,
    resolve::{Resolution, resolve_on_screen},
    response,
    screen::{Depth, Screen, difference, fingerprint, label, signature},
};

/// Consecutive unchanged turns that end a run as stalled.
const STALL_TURNS: u32 = 3;
/// Failures on one element before it is no longer offered.
const BAN_AFTER: u32 = 2;
/// Upper bound on `RunGoalRequest::max_retries`.
const MAX_RETRIES: u32 = 5;
/// Changed labels listed per history line.
const HISTORY_CHANGES: usize = 6;

pub(super) async fn run_goal_with<B: AgentBackend>(
    backend: B,
    runtime: JevRuntime,
    request: RunGoalRequest,
) -> DesktopResponse {
    let mut run = GoalRun::new(&request);
    let mut texts = request.text.clone().into_iter();
    let mut next_text = texts.next();
    let mut root = request.root.clone();
    let mut current: Option<Screen> = None;

    loop {
        if let Some(stop) = run.budget_exhausted() {
            return run.finish(stop, None);
        }
        let before = match current.take() {
            Some(screen) => screen,
            None => match observe_async(
                backend.clone(),
                request.app.clone(),
                root.clone(),
                run.depth,
            )
            .await
            {
                Ok(screen) => screen,
                Err(error) => return *error,
            },
        };
        let outcome = resolve_on_screen(
            &backend,
            &runtime,
            &before,
            Resolution {
                intent: &request.goal,
                text: next_text.as_deref(),
                include_values: request.include_values,
                execute: true,
                history: &run.history,
                allow_rerank: run.metrics.calls.saturating_add(1) < run.max_calls,
                banned: &run.banned,
            },
        )
        .await;
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => return *error,
        };
        run.charge(&outcome.evaluations);
        let decision = outcome.decision;
        let target_label = outcome
            .selected
            .as_ref()
            .map_or_else(|| "the selected element".to_owned(), label);
        if let Some(failure) = outcome.action_failure {
            if run.failed(
                &decision,
                outcome.selected.as_ref(),
                &target_label,
                &failure,
            ) {
                continue;
            }
            return run.finish(JevStopReason::ActionFailed, Some(decision));
        }
        if decision.decision == JevDecisionKind::Abstain && run.abstained(&decision) {
            root = None;
            current = Some(before);
            continue;
        }
        if let Some(stop) = stop_reason(decision.decision) {
            return run.finish(stop, Some(decision));
        }
        if decision.operation == JevOperation::TypeText {
            next_text = texts.next();
        }
        if decision.operation == JevOperation::Drill {
            root = decision.target.as_ref().map(|target| target.ref_id.clone());
        } else if decision.operation == JevOperation::Widen {
            root = None;
        }
        let observed = observe_async(
            backend.clone(),
            request.app.clone(),
            root.clone(),
            run.depth,
        )
        .await;
        match run.settle(&before, observed, decision, &target_label) {
            Ok(after) => current = Some(after),
            Err(stopped) => return *stopped,
        }
    }
}

/// Mutable state of one goal run.
struct GoalRun {
    turns: Vec<JevTurn>,
    history: Vec<String>,
    metrics: JevMetrics,
    unchanged: u32,
    retries: u32,
    max_retries: u32,
    max_steps: u32,
    max_calls: u32,
    depth: Depth,
    strikes: BTreeMap<String, u32>,
    banned: BTreeSet<String>,
}

impl GoalRun {
    fn new(request: &RunGoalRequest) -> Self {
        Self {
            turns: Vec::new(),
            history: Vec::new(),
            metrics: JevMetrics::default(),
            unchanged: 0,
            retries: 0,
            max_retries: request.max_retries.min(MAX_RETRIES),
            max_steps: request.max_steps.clamp(1, 40),
            max_calls: request.max_model_calls.clamp(1, 80),
            depth: if request.skeleton {
                Depth::Skeleton
            } else {
                Depth::Full
            },
            strikes: BTreeMap::new(),
            banned: BTreeSet::new(),
        }
    }

    fn charge(&mut self, evaluations: &[tinyjevclient::EvaluationResult]) {
        for evaluation in evaluations {
            merge_metrics(&mut self.metrics, evaluation);
        }
    }

    /// Records a completed action against the screen it produced, or ends
    /// the run when that screen is unreadable or the run has stalled.
    fn settle(
        &mut self,
        before: &Screen,
        observed: Result<Screen, Box<DesktopResponse>>,
        decision: JevDecision,
        target_label: &str,
    ) -> Result<Screen, Box<DesktopResponse>> {
        let Ok(after) = observed else {
            self.record_failure(&decision, "the screen could not be read after acting");
            return Err(Box::new(
                self.finish_ref(JevStopReason::ActionFailed, Some(decision)),
            ));
        };
        let navigated = matches!(
            decision.operation,
            JevOperation::Drill | JevOperation::Widen
        );
        let changed = navigated || fingerprint(&after) != fingerprint(before);
        let note = change_note(before, &after, changed);
        self.record(&decision, changed, target_label, note);
        if self.unchanged >= STALL_TURNS {
            return Err(Box::new(self.finish_ref(JevStopReason::Stalled, None)));
        }
        Ok(after)
    }

    fn budget_exhausted(&self) -> Option<JevStopReason> {
        if u32::try_from(self.turns.len()).unwrap_or(u32::MAX) >= self.max_steps {
            return Some(JevStopReason::ActionBudget);
        }
        (self.metrics.calls >= self.max_calls).then_some(JevStopReason::ModelBudget)
    }

    /// Spends one retry, reporting whether one was left.
    fn retry(&mut self) -> bool {
        if self.retries >= self.max_retries {
            return false;
        }
        self.retries = self.retries.saturating_add(1);
        true
    }

    /// Records a failed action; `true` when a retry remains.
    fn failed(
        &mut self,
        decision: &JevDecision,
        selected: Option<&super::screen::Candidate>,
        target_label: &str,
        failure: &DesktopResponse,
    ) -> bool {
        let code = failure
            .error
            .as_ref()
            .map_or("ACTION_FAILED", |error| error.code.as_str());
        if let Some(selected) = selected {
            self.strike(&signature(selected));
        }
        let note = format!("{:?} {target_label} failed with {code}", decision.operation);
        self.record_failure(decision, &note);
        self.retry()
    }

    /// Records a low-confidence turn; `true` when a retry remains.
    fn abstained(&mut self, decision: &JevDecision) -> bool {
        if !self.retry() {
            return false;
        }
        self.history.push(format!(
            "turn {}: no confident target (confidence {:.2}); look for a different element, scroll, or drill",
            self.turns.len() + 1,
            decision.confidence
        ));
        true
    }

    fn strike(&mut self, element: &str) {
        let strikes = self.strikes.entry(element.to_owned()).or_default();
        *strikes = strikes.saturating_add(1);
        if *strikes >= BAN_AFTER {
            self.banned.insert(element.to_owned());
        }
    }

    fn next_step(&self) -> u32 {
        u32::try_from(self.turns.len())
            .unwrap_or(u32::MAX)
            .saturating_add(1)
    }

    fn record_failure(&mut self, decision: &JevDecision, note: &str) {
        let step = self.next_step();
        self.history.push(format!("step {step}: {note}"));
        self.turns.push(JevTurn {
            step,
            operation: decision.operation,
            target: decision.target.clone(),
            confidence: decision.confidence,
            ok: false,
            changed: false,
            note: note.to_owned(),
        });
    }

    fn record(&mut self, decision: &JevDecision, changed: bool, target: &str, note: String) {
        self.unchanged = if changed {
            0
        } else {
            self.unchanged.saturating_add(1)
        };
        let step = self.next_step();
        self.history.push(format!(
            "step {step}: {:?} {target}; {note}",
            decision.operation
        ));
        self.turns.push(JevTurn {
            step,
            operation: decision.operation,
            target: decision.target.clone(),
            confidence: decision.confidence,
            ok: decision.executed,
            changed,
            note,
        });
    }

    fn finish(self, stop: JevStopReason, pending: Option<JevDecision>) -> DesktopResponse {
        self.finish_ref(stop, pending)
    }

    fn finish_ref(&self, stop: JevStopReason, pending: Option<JevDecision>) -> DesktopResponse {
        response(
            "run-goal",
            &JevRunResult {
                stop,
                turns: self.turns.clone(),
                pending,
                metrics: self.metrics.clone(),
            },
        )
    }
}

/// Describes what an action changed, in labels Jev can match next turn.
pub(super) fn change_note(before: &Screen, after: &Screen, changed: bool) -> String {
    if !changed {
        return "nothing on screen changed".to_owned();
    }
    let (appeared, disappeared) = difference(before, after);
    let mut parts = Vec::new();
    if before.window != after.window {
        parts.push(format!(
            "window is now {:?}",
            after.window.as_deref().unwrap_or_default()
        ));
    }
    if before.surface != after.surface {
        parts.push(format!("surface is now {}", after.surface));
    }
    if !appeared.is_empty() {
        parts.push(format!("appeared: {}", summarize(&appeared)));
    }
    if !disappeared.is_empty() {
        parts.push(format!("gone: {}", summarize(&disappeared)));
    }
    if parts.is_empty() {
        "the screen changed".to_owned()
    } else {
        parts.join("; ")
    }
}

fn summarize(labels: &[String]) -> String {
    let mut shown = labels
        .iter()
        .take(HISTORY_CHANGES)
        .cloned()
        .collect::<Vec<_>>();
    if labels.len() > HISTORY_CHANGES {
        shown.push(format!("and {} more", labels.len() - HISTORY_CHANGES));
    }
    shown.join(", ")
}

fn stop_reason(decision: JevDecisionKind) -> Option<JevStopReason> {
    match decision {
        JevDecisionKind::Done => Some(JevStopReason::Done),
        JevDecisionKind::Blocked => Some(JevStopReason::Blocked),
        JevDecisionKind::ConfirmationRequired => Some(JevStopReason::ConfirmationRequired),
        JevDecisionKind::Abstain => Some(JevStopReason::LowConfidence),
        JevDecisionKind::NeedsText => Some(JevStopReason::NeedsText),
        JevDecisionKind::Act => None,
    }
}
