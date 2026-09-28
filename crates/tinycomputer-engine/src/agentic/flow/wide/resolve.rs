//! Settling what a wide request prepared: confirming a hesitant target,
//! choosing among a knockout's winners, and dismissing an obstacle.

use tinycomputer_bus::{FlowLoop, JevOperation};

use crate::agentic::flow::{
    AgentBackend, FlowRun, Halt, StepLog,
    ask::{self, Questions, corroborate, probability},
    ground::{AGREED, CORROBORATED, Grounded},
    memory::{learn, remember},
    view::{Screen, label},
};

use super::{Dismissal, Prepared, obstacle_key};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Settles a prepared target: used as it is, confirmed first, or chosen
    /// among a knockout's winners.
    pub(in crate::agentic::flow) async fn resolve(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        purpose: &str,
        prepared: &Prepared,
    ) -> Result<Option<Grounded>, Halt> {
        match prepared {
            Prepared::Chosen(grounded) => Ok(Some(grounded.clone())),
            Prepared::Nothing => Ok(None),
            Prepared::Finals(winners) => {
                self.decide(log, screen, purpose, winners.clone(), None)
                    .await
            }
            Prepared::Unsure {
                candidate,
                confidence,
                consistency,
            } => {
                if consistency.is_some() {
                    log.used(FlowLoop::Consistency);
                }
                let corroboration = self.enabled(FlowLoop::Corroboration);
                if consistency.is_none() && !corroboration {
                    return Ok(None);
                }
                let confirm = if corroboration {
                    log.used(FlowLoop::Corroboration);
                    let answers = self
                        .ask(
                            log,
                            ask::request(
                                self.model(),
                                self.state(screen, purpose),
                                Questions::default().with(
                                    "confirm",
                                    corroborate(purpose, candidate, self.include_values),
                                ),
                            ),
                        )
                        .await?;
                    probability(&answers, "confirm").unwrap_or_default()
                } else {
                    0.0
                };
                let agrees = consistency.unwrap_or(false);
                let accepted = match (consistency.is_some(), corroboration) {
                    (true, true) => (agrees && confirm >= AGREED) || confirm >= CORROBORATED,
                    (true, false) => agrees,
                    (false, _) => confirm >= CORROBORATED,
                };
                Ok(accepted.then(|| Grounded {
                    candidate: candidate.clone(),
                    confidence: confidence.max(confirm),
                }))
            }
        }
    }

    /// Clears what is in front the way the wide request chose, and
    /// remembers which control did it.
    pub(in crate::agentic::flow) async fn dismiss(
        &mut self,
        log: &mut StepLog,
        dismissal: Dismissal,
    ) -> Result<(), Halt> {
        let Some(target) = dismissal.control else {
            let app = self.app.clone();
            self.act(log, "press escape (dismiss)", None, move |backend| {
                backend.press(&app, "escape")
            })
            .await?;
            self.history
                .push(format!("pressed escape to dismiss {}", dismissal.front));
            self.ledger
                .tried(format!("already pressed escape on {}", dismissal.front));
            return Ok(());
        };
        let chosen_target = target.clone();
        let reply = self
            .act(log, "click (dismiss)", Some(&target), move |backend| {
                backend.execute(JevOperation::Click, Some(chosen_target), None)
            })
            .await?;
        self.history.push(format!(
            "dismissed {} with {}",
            dismissal.front,
            label(&target)
        ));
        self.ledger.tried(format!(
            "already dismissed {} with {}",
            dismissal.front,
            label(&target)
        ));
        if reply.ok {
            learn(
                &mut self.learned,
                remember(&self.app, &obstacle_key(&dismissal.front), &target),
            );
        }
        Ok(())
    }
}
