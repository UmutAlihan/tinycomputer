//! The `stop_before` step: finding an irreversible control and stopping in
//! front of it, or pressing it when the run allows destructive actions.

use tinycomputer_bus::{FlowLoop, FlowStopReason, JevOperation, StepOutcome};

use crate::agentic::flow::{
    Ended, FlowRun, Halt, StepLog,
    act::DONE,
    backend::AgentBackend,
    memory::{learn, remember},
    view::{label, target_payload},
};

use super::{IRREVERSIBLE_FLOOR, LOCATE_FLOOR, matching::clickable};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    pub(super) async fn stop_before(
        &mut self,
        log: &mut StepLog,
        action: &str,
    ) -> Result<Ended, Halt> {
        let purpose = format!("perform: {action}");
        // Asked to "perform: paying", Jev weighs the request against the
        // brief's own rule to stop before paying and hesitates (measured:
        // 0.44 on the Pay button); asked to find it without pressing it,
        // which is all a gated step does, it answers 1.0.
        let question = if self.allow_destructive {
            purpose.clone()
        } else {
            format!("find, without pressing it, the control that would perform: {action}")
        };
        let screen = self.look().await?;
        let pool = clickable(&screen.candidates);
        let Some(grounded) = self
            .ground(log, &screen, &question, &purpose, pool)
            .await?
            .filter(|grounded| grounded.confidence >= LOCATE_FLOOR)
        else {
            return Err(Halt::Failed(format!(
                "the control that performs {action:?} was not found"
            )));
        };
        log.confidence = Some(grounded.confidence);
        let target = grounded.candidate;
        if !self.allow_destructive {
            self.pending = Some(target_payload(&target));
            self.history.push(format!(
                "found {} for {action:?} and stopped in front of it",
                label(&target)
            ));
            return Err(Halt::Stop(FlowStopReason::StoppedBeforeDestructive));
        }
        if self.deep()
            && self.deliberates(FlowLoop::Evidence)
            && grounded.confidence < IRREVERSIBLE_FLOOR
        {
            // Nothing undoes this press: the bar is higher than for any
            // other, and a pick short of it is vouched for once more.
            let vouched = self.vouch(log, &screen, &purpose, &target).await?;
            log.confidence = Some(vouched);
            if vouched < IRREVERSIBLE_FLOOR {
                return Err(Halt::Failed(format!(
                    "will not press {} irreversibly on uncertain evidence (confidence {vouched:.2})",
                    label(&target)
                )));
            }
        }
        let clicked = target.clone();
        let reply = self
            .act(log, "click (irreversible)", Some(&target), move |backend| {
                backend.execute(JevOperation::Click, Some(clicked), None)
            })
            .await?;
        if !reply.ok {
            return Err(Halt::Failed(format!(
                "{} could not be pressed",
                label(&target)
            )));
        }
        learn(&mut self.learned, remember(&self.app, &purpose, &target));
        let happened = self.holds(log, &format!("{action} has happened")).await?;
        if happened >= DONE {
            Ok(Ended::new(
                StepOutcome::Done,
                format!("performed {action:?}"),
            ))
        } else {
            Err(Halt::Failed(format!(
                "pressed {} but {action:?} is not visibly done (confidence {happened:.2})",
                label(&target)
            )))
        }
    }
}
