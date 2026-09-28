//! Steps that judge a condition on screen: `verify`, `wait_for`,
//! `repeat_until`, and `if`.

use tinycomputer_bus::{FlowLoop, IfStep, JevOperation, RepeatStep, StepOutcome};

use crate::agentic::flow::{
    Ended, FlowRun, Halt, StepLog,
    act::{DONE, SCREEN_VIEW},
    ask::{self, Questions, condition},
    backend::AgentBackend,
    escalate::Belief,
    validate::{MAX_REPEAT, substitute_safe},
};

use super::WAIT_CHECKS;

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Judges one condition on the current screen.
    ///
    /// A deliberating run settles a judgement near [`DONE`] on its evidence
    /// (`escalate::settle_belief`), at the deep level also asking it over
    /// the screen alone, without the history that can lead it.
    pub(in crate::agentic::flow) async fn holds(
        &mut self,
        log: &mut StepLog,
        condition_text: &str,
    ) -> Result<f64, Halt> {
        log.used(FlowLoop::Completion);
        let screen = self.look().await?;
        let request = ask::request(
            self.model(),
            self.state(&screen, condition_text),
            Questions::default()
                .with("holds", condition(condition_text))
                .with("negated", ask::negated(condition_text))
                .with("coverage", ask::coverage(condition_text)),
        );
        let mut answers = self.ask(log, request.clone()).await?;
        let belief = Belief {
            site: "holds",
            yes: "holds",
            no: "negated",
            top: Some("coverage"),
            threshold: DONE,
        };
        let views = if self.deep() {
            vec![ask::request(
                self.model(),
                ask::state(&screen, condition_text, &[], self.include_values),
                Questions::default()
                    .with("holds", ask::viewed(condition(condition_text), SCREEN_VIEW))
                    .with(
                        "negated",
                        ask::viewed(ask::negated(condition_text), SCREEN_VIEW),
                    ),
            )]
        } else {
            Vec::new()
        };
        let held = self
            .settle_belief(log, belief, &request, &mut answers, views)
            .await?
            .unwrap_or_default();
        log.confidence = Some(held);
        Ok(held)
    }

    pub(super) async fn verify(
        &mut self,
        log: &mut StepLog,
        condition_text: &str,
    ) -> Result<Ended, Halt> {
        let held = self.holds(log, condition_text).await?;
        if held >= DONE {
            Ok(Ended::new(
                StepOutcome::Done,
                format!("holds (confidence {held:.2})"),
            ))
        } else {
            Err(Halt::Failed(format!(
                "does not hold (confidence {held:.2})"
            )))
        }
    }

    pub(super) async fn wait_for(
        &mut self,
        log: &mut StepLog,
        condition_text: &str,
    ) -> Result<Ended, Halt> {
        for check in 0..WAIT_CHECKS {
            let held = self.holds(log, condition_text).await?;
            if held >= DONE {
                return Ok(Ended::new(
                    StepOutcome::Done,
                    format!("held after {} check(s)", check + 1),
                ));
            }
            self.act(log, "wait", None, |backend| {
                backend.execute(JevOperation::Wait, None, None)
            })
            .await?;
        }
        Err(Halt::Failed(format!(
            "still not true after {WAIT_CHECKS} checks"
        )))
    }

    pub(super) async fn repeat(
        &mut self,
        log: &mut StepLog,
        repeat: &RepeatStep,
        path: &str,
    ) -> Result<Ended, Halt> {
        let condition_text = substitute_safe(&repeat.condition, &self.vars, &self.facts);
        for round in 0..repeat.max.min(MAX_REPEAT) {
            if self.holds(log, &condition_text).await? >= DONE {
                return Ok(Ended::new(
                    StepOutcome::Done,
                    format!("held after {round} round(s)"),
                ));
            }
            self.run_steps(&repeat.steps, format!("{path}.r{}", round + 1))
                .await?;
            // The last child left `self.step` at its own nested path; restore
            // it before the next `holds` check so that call, and the final
            // one below on the last round, are traced to this repeat_until
            // step rather than misattributed to the child that just ran.
            path.clone_into(&mut self.step);
        }
        if self.holds(log, &condition_text).await? >= DONE {
            return Ok(Ended::new(StepOutcome::Done, "held after the last round"));
        }
        Err(Halt::Failed(format!(
            "still not true after {} round(s)",
            repeat.max.min(MAX_REPEAT)
        )))
    }

    pub(super) async fn branch(
        &mut self,
        log: &mut StepLog,
        branch: &IfStep,
        path: &str,
    ) -> Result<Ended, Halt> {
        let condition_text = substitute_safe(&branch.condition, &self.vars, &self.facts);
        let held = self.holds(log, &condition_text).await?;
        let (steps, taken) = if held >= DONE {
            (&branch.then, "then")
        } else {
            (&branch.otherwise, "else")
        };
        self.run_steps(steps, path.to_owned()).await?;
        Ok(Ended::new(
            StepOutcome::Done,
            format!("took the {taken} branch (confidence {held:.2})"),
        ))
    }
}
