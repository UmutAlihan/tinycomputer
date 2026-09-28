//! Performing a goal turn's action on a freshly re-found target, then
//! observing what it changed and whether the goal is now visibly done.

use std::time::{Duration, Instant};

use tinycomputer_bus::{DesktopResponse, JevDecision, JevOperation, JevStopReason};

use super::super::backend::{AgentBackend, execute_operation};
use super::super::goal::{
    mutates, prepared_text, record_turn, same_target, selected_target, target_allowed,
    within_scope,
};
use super::super::reply::{action_failed_response, failed_turn};
use super::super::screen::{Candidate, Screen, fingerprint};
use super::super::verify::{satisfied, verify};
use super::GoalLoop;

impl<B: AgentBackend> GoalLoop<B> {
    pub(super) async fn execute_step(
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
                .filter(|label| super::super::verify::exact_label(&current, label))
                .any(|label| {
                    fresh
                        .candidates
                        .iter()
                        .filter(|candidate| super::super::verify::exact_label(candidate, label))
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
        if delivered_unverified && decision.destructive >= super::super::policy::DESTRUCTIVE {
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
