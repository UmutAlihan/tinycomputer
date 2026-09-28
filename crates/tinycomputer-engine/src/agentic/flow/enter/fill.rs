//! Delivering slots: filling every pending slot a field or an option can
//! take, each text verified by reading it back.

use std::collections::BTreeSet;

use serde_json::Value;
use tinycomputer_bus::Slot;
use tinycomputer_core::reformat_date;

use crate::agentic::flow::{
    AgentBackend, FlowRun, Halt, StepLog,
    backend::deliver_text,
    memory::{learn, remember},
    view::{Candidate, element_kind, label},
};

use super::{BLIND_PICK_MISSES, REVEAL_TURNS, editable, names};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Fills every slot in `pending` it can find a field or an option for,
    /// removing each one that arrives.
    pub(super) async fn fill_pending(
        &mut self,
        log: &mut StepLog,
        slots: &[Slot],
        private: &[bool],
        pending: &mut BTreeSet<usize>,
    ) -> Result<(), Halt> {
        let mut revealed = false;
        let mut saw_fields = false;
        // Fields that refused the text this step: a `div` a page labels a
        // combobox, or a field that would not hold what was typed. Offered
        // again, the same wrong field wins again — struck by `element_kind`
        // so the rows of a city list, each a same-kind box that differs only
        // by the city it holds, are struck together rather than one at a
        // time (`a_row_that_refused_the_text_is_never_pressed_while_revealing_a_field`).
        let mut struck: BTreeSet<String> = BTreeSet::new();
        for _ in 0..3 {
            if pending.is_empty() {
                break;
            }
            let mut screen = self.look().await?;
            if editable(&screen).len() < pending.len() && !screen.unexplored.is_empty() {
                self.explore(&mut screen).await;
            }
            let fields = editable(&screen)
                .into_iter()
                .filter(|field| !struck.contains(&element_kind(field)))
                .collect::<Vec<_>>();
            saw_fields |= !fields.is_empty();
            let assignments = if fields.is_empty() {
                Vec::new()
            } else {
                self.assign(log, &screen, slots, pending, &fields).await?
            };
            if assignments.is_empty() {
                if revealed {
                    break;
                }
                revealed = true;
                let reveal = if fields.is_empty() {
                    format!("show the editable fields for: {}", names(slots, pending))
                } else {
                    format!("show the fields for: {}", names(slots, pending))
                };
                // A field that cannot be revealed is looked for another way
                // below, or found not to be asked for; it is not a failure.
                match self.accomplish(log, &reveal, REVEAL_TURNS).await {
                    Err(Halt::Failed(note)) => self
                        .history
                        .push(format!("could not reveal the fields ({note})")),
                    other => {
                        other?;
                    }
                }
                continue;
            }
            for assignment in assignments {
                let slot = &slots[assignment.slot];
                let filled = self
                    .fill(log, slot, &assignment.field, &screen.context)
                    .await?;
                if !filled {
                    struck.insert(element_kind(&assignment.field));
                    self.refused.insert(element_kind(&assignment.field));
                    self.ledger.tried(format!(
                        "{} did not take the {}",
                        label(&assignment.field),
                        slot.slot
                    ));
                }
                if filled {
                    pending.remove(&assignment.slot);
                    learn(
                        &mut self.learned,
                        remember(&self.app, &slot.slot, &assignment.field),
                    );
                    log.confidence = Some(log.confidence.map_or(assignment.probability, |seen| {
                        seen.min(assignment.probability)
                    }));
                }
            }
        }
        // A value with no field to type into is picked instead, as a date
        // from a calendar or a city from a list of suggestions. Only a "the
        // value was not found" failure is safe to shrug off and move to the
        // next slot; a budget stop or a backend error means acting further
        // is unsafe or pointless, and must end the step instead of being
        // read as "this slot has no picker".
        let mut missed = 0;
        for index in pending.clone() {
            if !saw_fields && missed >= BLIND_PICK_MISSES {
                self.history.push(format!(
                    "no field and no picker on this screen; not looking for: {}",
                    names(slots, pending)
                ));
                break;
            }
            let slot = &slots[index];
            match self
                .pick_option(log, &slot.slot, &slot.text, private[index], false)
                .await
            {
                Ok(_) => {
                    pending.remove(&index);
                }
                Err(Halt::Failed(_)) => missed += 1,
                Err(halt) => return Err(halt),
            }
        }
        Ok(())
    }

    /// Delivers one slot's text and reports whether it verifiably arrived.
    ///
    /// A date is typed in the layout the field or the page around it asks
    /// for ("DD-MM-YYYY"), so an input mask does not mangle it.
    async fn fill(
        &mut self,
        log: &mut StepLog,
        slot: &Slot,
        field: &Candidate,
        context: &[String],
    ) -> Result<bool, Halt> {
        let app = self.app.clone();
        let target = field.clone();
        let hints = [field.name.as_deref(), field.description.as_deref()]
            .into_iter()
            .flatten()
            .chain(context.iter().map(String::as_str));
        let text = reformat_date(&slot.text, hints).unwrap_or_else(|| slot.text.clone());
        let reply = self
            .act(
                log,
                &format!("fill {}", slot.slot),
                Some(field),
                move |backend| deliver_text(&backend, &app, &target, &text),
            )
            .await?;
        self.typed.insert(element_kind(field));
        let path = reply
            .data
            .as_ref()
            .and_then(|data| data.get("path"))
            .cloned()
            .unwrap_or(Value::Null);
        self.history.push(format!(
            "entered the {} into {} ok={} via {path}",
            slot.slot,
            label(field),
            reply.ok
        ));
        Ok(reply.ok)
    }
}
