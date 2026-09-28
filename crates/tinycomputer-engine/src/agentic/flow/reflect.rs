//! Reflection: after a `choose` pressed something, check the screen shows
//! the choice it asked for, and repair it once when it does not.
//!
//! A press can succeed and still leave the wrong thing: a stepper whose label
//! names "1 Adult" adds a second adult. `docs/specs/flow-reflection.md` is the
//! contract.

use serde_json::json;
use tinycomputer_bus::{FlowAction, FlowLoop, StepOutcome};

use super::{
    AgentBackend, Ended, FlowRun, Halt, StepLog,
    ask::{self, Questions},
};

/// Least calibrated belief that a choice was made as asked, below which the
/// step is repaired, and failed when the repair does not take.
pub(super) const REFLECT_FLOOR: f64 = 0.5;
/// Most `do` turns one repair may take.
pub(super) const REPAIR_TURNS: u32 = 4;

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// `result` of the step `action`, reflected on when it is a `choose`
    /// that ended `Done` after pressing something: a press can succeed and
    /// leave the wrong choice.
    pub(super) async fn reflected(
        &mut self,
        log: &mut StepLog,
        action: &FlowAction,
        text: &str,
        result: Result<Ended, Halt>,
    ) -> Result<Ended, Halt> {
        match result {
            Ok(ended)
                if matches!(action, FlowAction::Choose(_))
                    && ended.outcome == StepOutcome::Done
                    && !log.actions.is_empty() =>
            {
                self.reflect(log, text).await.map(|()| ended)
            }
            other => other,
        }
    }

    /// Reflects on the `choose` step `intent` that just pressed something:
    /// `Ok` when the screen shows its choice, first time or after one
    /// repair; the step fails otherwise.
    async fn reflect(&mut self, log: &mut StepLog, intent: &str) -> Result<(), Halt> {
        let held = self.reflection(log, intent, "first").await?;
        if held >= REFLECT_FLOOR {
            return Ok(());
        }
        self.history.push(format!(
            "reflection: the screen does not show the step's choice ({intent}); correcting it"
        ));
        let repair = format!(
            "correct the previous step so the screen shows: choose {intent}; undo anything it changed that was not asked for"
        );
        // A repair that fails is judged by the second reflection like one
        // that ran out of turns: what matters is what the screen shows.
        match self.accomplish(log, &repair, REPAIR_TURNS).await {
            Ok(_) | Err(Halt::Failed(_)) => {}
            Err(other) => return Err(other),
        }
        let held = self.reflection(log, intent, "after_repair").await?;
        if held >= REFLECT_FLOOR {
            return Ok(());
        }
        Err(Halt::Failed(format!(
            "reflection: the screen still does not show {intent} (confidence {held:.2})"
        )))
    }

    /// How strongly Jev believes the screen shows the choice `intent`
    /// asked for; `1.0` when it gives no answer, so silence never fails a
    /// step.
    async fn reflection(
        &mut self,
        log: &mut StepLog,
        intent: &str,
        attempt: &str,
    ) -> Result<f64, Halt> {
        log.used(FlowLoop::Reflection);
        let screen = self.look().await?;
        let answers = self
            .ask(
                log,
                ask::request(
                    self.model(),
                    self.state(&screen, &format!("reflect on: choose {intent}")),
                    Questions::default()
                        .with("reflects", ask::reflects(&format!("choose {intent}")))
                        .with("strays", ask::strays(&format!("choose {intent}"))),
                ),
            )
            .await?;
        let held = ask::calibrated(&answers, "reflects", "strays").unwrap_or(1.0);
        self.runtime.journal.record(
            "reflect",
            || json!({"step": self.step, "held": held, "attempt": attempt}),
        );
        Ok(held)
    }
}
