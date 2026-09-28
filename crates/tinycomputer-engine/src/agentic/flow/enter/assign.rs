//! Matching slots to fields: which slots the form on screen asks for, which
//! it flags with an error, and which field takes each slot.

use std::collections::BTreeSet;

use serde_json::Value;
use tinycomputer_bus::{FlowLoop, Slot, StepOutcome};
use tinycomputer_core::reformat_date;

use crate::agentic::flow::{
    AgentBackend, Ended, FlowRun, Halt, StepLog,
    ask::{self, CAP, Questions, asks_for, chosen, elements, field_error, numbered, probability},
    backend::deliver_text,
    memory::{learn, recall, remember},
    validate::{references, substitute, substitute_safe},
    view::{Candidate, Screen, element_kind, label, signature},
};

use super::{SLOT_FLOOR, REVEAL_TURNS, FIELD_ERROR, NOT_ASKED, BLIND_PICK_MISSES, Assignment, names, editable, position};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {

    /// The `pending` slots the form on screen does not ask for, by one Noul
    /// each.
    pub(super) async fn unasked(
        &mut self,
        log: &mut StepLog,
        slots: &[Slot],
        pending: &BTreeSet<usize>,
    ) -> Result<BTreeSet<usize>, Halt> {
        let screen = self.look().await?;
        let mut questions = Questions::default();
        for index in pending {
            questions = questions.with(&format!("asks_{index}"), asks_for(&slots[*index].slot));
        }
        let answers = self
            .ask(
                log,
                ask::request(
                    self.model(),
                    self.state(&screen, "check which details the form asks for"),
                    questions,
                ),
            )
            .await?;
        Ok(pending
            .iter()
            .copied()
            .filter(|index| {
                probability(&answers, &format!("asks_{index}")).is_some_and(|asks| asks < NOT_ASKED)
            })
            .collect())
    }

    /// The slots the screen shows an error about, by one Noul each.
    pub(super) async fn flagged(
        &mut self,
        log: &mut StepLog,
        slots: &[Slot],
    ) -> Result<BTreeSet<usize>, Halt> {
        log.used(FlowLoop::Validation);
        let screen = self.look().await?;
        let mut questions = Questions::default();
        for (index, slot) in slots.iter().enumerate() {
            questions = questions.with(&format!("error_{index}"), field_error(&slot.slot));
        }
        let answers = self
            .ask(
                log,
                ask::request(
                    self.model(),
                    self.state(&screen, "check the entered form for errors"),
                    questions,
                ),
            )
            .await?;
        Ok((0..slots.len())
            .filter(|index| {
                probability(&answers, &format!("error_{index}")).unwrap_or_default() >= FIELD_ERROR
            })
            .collect())
    }

    /// Matches the pending slots to fields, one field per slot.
    pub(super) async fn assign(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        slots: &[Slot],
        pending: &BTreeSet<usize>,
        fields: &[Candidate],
    ) -> Result<Vec<Assignment>, Halt> {
        let mut assignments = Vec::new();
        let mut open = Vec::new();
        for index in pending {
            let slot = &slots[*index].slot;
            if self.enabled(FlowLoop::Memory)
                && let Some(known) = recall(&self.memory, &self.app, slot, fields)
            {
                log.used(FlowLoop::Memory);
                assignments.push(Assignment {
                    slot: *index,
                    field: known.clone(),
                    probability: 1.0,
                });
            } else {
                open.push(*index);
            }
        }
        let taken = assignments
            .iter()
            .map(|assignment| signature(&assignment.field))
            .collect::<BTreeSet<_>>();
        // Unlike a click or expand target, two fields that describe alike
        // are not interchangeable: split date parts and card expiry MM/YY
        // boxes are the same shape but hold different text. `distinct`
        // would collapse them to one offered choice — position in the pool
        // is what tells them apart, so no lookalike field is deduplicated
        // away. `CAP` below still limits how many fields one request offers.
        let offered = fields
            .iter()
            .filter(|field| !taken.contains(&signature(field)))
            .take(CAP)
            .cloned()
            .collect::<Vec<_>>();
        if !open.is_empty() && !offered.is_empty() {
            let keys = numbered(offered.len());
            let mut questions = Questions::default();
            for index in &open {
                questions = questions.with(
                    &format!("slot_{index}"),
                    elements(
                        &format!("type the {} into it", slots[*index].slot),
                        &offered,
                        &keys,
                        self.include_values,
                    ),
                );
            }
            let purpose = open
                .iter()
                .map(|index| slots[*index].slot.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let answers = self
                .ask(
                    log,
                    ask::request(
                        self.model(),
                        self.state(screen, &format!("fill in: {purpose}")),
                        questions,
                    ),
                )
                .await?;
            let mut proposals = open
                .iter()
                .filter_map(|index| {
                    let (choice, probability) = chosen(&answers, &format!("slot_{index}"))?;
                    let position = keys.iter().position(|key| *key == choice)?;
                    Some(Assignment {
                        slot: *index,
                        field: offered.get(position)?.clone(),
                        probability,
                    })
                })
                .filter(|assignment| assignment.probability >= SLOT_FLOOR)
                .collect::<Vec<_>>();
            proposals.sort_by(|left, right| right.probability.total_cmp(&left.probability));
            let mut claimed = taken;
            for proposal in proposals {
                if claimed.insert(signature(&proposal.field)) {
                    assignments.push(proposal);
                }
            }
        }
        assignments.sort_by(|left, right| position(&left.field).total_cmp(&position(&right.field)));
        Ok(assignments)
    }

}
