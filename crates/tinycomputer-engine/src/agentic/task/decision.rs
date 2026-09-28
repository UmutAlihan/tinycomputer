//! Turning one Jev decision into what the goal loop does next: act, ask
//! again, stop, or hold the action for the caller's confirmation.

use std::time::Instant;

use tinycomputer_bus::{DesktopResponse, JevDecision, JevStopReason, RunGoalRequest};

use super::super::backend::AgentBackend;
use super::super::goal::{selected_target, stop_reason};
use super::super::pending::{PendingRun, queue_confirmation};
use super::super::screen::Screen;
use super::{DecisionFlow, GoalLoop};

impl<B: AgentBackend> GoalLoop<B> {
    pub(super) fn handle_decision(
        &mut self,
        before: &Screen,
        decision: &JevDecision,
    ) -> DecisionFlow {
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
                journal: self.runtime.journal.clone(),
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
}
