//! The slot matcher behind `enter`: which field takes which text.
//!
//! All slots are matched in one request, one Choice per slot over the same
//! editable fields, and assigned greedily by probability so two slots can never
//! claim one field. Each text is then delivered with read-back verification
//! (`deliver_text`), top to bottom in screen order. A slot with no visible
//! field first runs a short `do` loop to reveal one.

use std::collections::BTreeSet;

use serde_json::Value;
use tinycomputer_bus::{FlowLoop, Slot, StepOutcome};
use tinycomputer_core::reformat_date;

use super::{
    AgentBackend, Ended, FlowRun, Halt, StepLog,
    ask::{self, CAP, Questions, asks_for, chosen, elements, field_error, numbered, probability},
    backend::deliver_text,
    memory::{learn, recall, remember},
    validate::{references, substitute, substitute_safe},
    view::{Candidate, Screen, distinct, element_kind, label, signature},
};

/// Least probability a slot assignment needs.
const SLOT_FLOOR: f64 = 0.4;
/// Turns spent revealing fields that are not on screen yet.
const REVEAL_TURNS: u32 = 4;
/// Probability of an error shown about a field that makes it entered again.
const FIELD_ERROR: f64 = 0.7;
/// Probability that a form asks for a detail, under which a detail with no
/// field is taken as not asked for rather than failing the step.
const NOT_ASKED: f64 = 0.35;

/// One slot matched to one field.
#[derive(Debug, Clone)]
struct Assignment {
    slot: usize,
    field: Candidate,
    probability: f64,
}

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Runs an `enter` step.
    pub(super) async fn enter(&mut self, log: &mut StepLog, slots: &[Slot]) -> Result<Ended, Halt> {
        log.used(FlowLoop::Slots);
        // A slot whose text names a secret is private: its value is never
        // written into a question, even as the option to pick.
        let private = slots
            .iter()
            .map(|slot| {
                references(&slot.text)
                    .iter()
                    .any(|name| self.facts.contains(name))
            })
            .collect::<Vec<_>>();
        let slots = slots
            .iter()
            .map(|slot| Slot {
                // The label names a field for Jev, so it must never carry a
                // fact's value; the text is typed into the field locally and
                // never shown, so it may.
                slot: substitute_safe(&slot.slot, &self.vars, &self.facts),
                text: substitute(&slot.text, &self.vars),
            })
            .collect::<Vec<_>>();
        let mut pending = (0..slots.len()).collect::<BTreeSet<_>>();
        self.fill_pending(log, &slots, &private, &mut pending)
            .await?;
        // A detail the form never asks for — a title where it only asks for
        // gender — has no field; that is not a failure.
        let mut unasked = BTreeSet::new();
        if !pending.is_empty() && self.enabled(FlowLoop::Validation) {
            unasked = self.unasked(log, &slots, &pending).await?;
            pending.retain(|index| !unasked.contains(index));
            if !unasked.is_empty() {
                self.history.push(format!(
                    "the form does not ask for: {}",
                    names(&slots, &unasked)
                ));
            }
        }
        if pending.is_empty() && self.enabled(FlowLoop::Validation) {
            // A form that rejects a value says so next to its field; enter
            // those once more, then give up naming them.
            let flagged = self.flagged(log, &slots).await?;
            if !flagged.is_empty() {
                self.history.push(format!(
                    "the form shows an error about: {}; entering those again",
                    names(&slots, &flagged)
                ));
                pending = flagged;
                self.fill_pending(log, &slots, &private, &mut pending)
                    .await?;
                let still = self.flagged(log, &slots).await?;
                if !still.is_empty() {
                    return Err(Halt::Failed(format!(
                        "the form still shows an error about: {}",
                        names(&slots, &still)
                    )));
                }
            }
        }
        if pending.is_empty() {
            self.remember_choice(&format!(
                "entered: {}",
                slots
                    .iter()
                    .map(|slot| slot.slot.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            let entered = slots.len() - unasked.len();
            Ok(Ended::new(
                StepOutcome::Done,
                if unasked.is_empty() {
                    format!("entered {entered} value(s)")
                } else {
                    format!(
                        "entered {entered} value(s); the form does not ask for: {}",
                        names(&slots, &unasked)
                    )
                },
            ))
        } else {
            let refused = if self.refused.is_empty() {
                String::new()
            } else {
                format!(
                    "; {} element(s) the page offered as fields refused the text",
                    self.refused.len()
                )
            };
            Err(Halt::Failed(format!(
                "no field that takes text was found for: {}{refused}",
                names(&slots, &pending)
            )))
        }
    }

    /// The `pending` slots the form on screen does not ask for, by one Noul
    /// each.
    async fn unasked(
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
    async fn flagged(
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

    /// Fills every slot in `pending` it can find a field or an option for,
    /// removing each one that arrives.
    async fn fill_pending(
        &mut self,
        log: &mut StepLog,
        slots: &[Slot],
        private: &[bool],
        pending: &mut BTreeSet<usize>,
    ) -> Result<(), Halt> {
        let mut revealed = false;
        // Fields that refused the text this step: a `div` a page labels a
        // combobox, or a field that would not hold what was typed. Offered
        // again, the same wrong field wins again.
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
        for index in pending.clone() {
            let slot = &slots[index];
            match self
                .pick_option(log, &slot.slot, &slot.text, private[index], false)
                .await
            {
                Ok(_) => {
                    pending.remove(&index);
                }
                Err(Halt::Failed(_)) => {}
                Err(halt) => return Err(halt),
            }
        }
        Ok(())
    }

    /// Matches the pending slots to fields, one field per slot.
    async fn assign(
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
        // is what tells them apart, so every field stays offered.
        let offered = fields
            .iter()
            .filter(|field| !taken.contains(&signature(field)))
            .cloned()
            .take(CAP)
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

/// The names of the slots at `indices`, joined.
fn names(slots: &[Slot], indices: &BTreeSet<usize>) -> String {
    indices
        .iter()
        .map(|index| slots[*index].slot.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Fields that accept text, top to bottom.
pub(super) fn editable(screen: &Screen) -> Vec<Candidate> {
    let mut fields = screen
        .candidates
        .iter()
        .filter(|candidate| {
            candidate
                .available_actions
                .iter()
                .any(|action| action == "SetValue" || action == "TypeText")
                || [
                    "textfield",
                    "textarea",
                    "text field",
                    "text area",
                    "combobox",
                    "searchfield",
                    "webarea",
                    "document",
                ]
                .iter()
                .any(|role| candidate.role.eq_ignore_ascii_case(role))
        })
        .cloned()
        .collect::<Vec<_>>();
    fields.sort_by(|left, right| position(left).total_cmp(&position(right)));
    fields
}

/// A reading-order key: rows top to bottom, then left to right.
fn position(candidate: &Candidate) -> f64 {
    let coordinate = |axis: &str| {
        candidate
            .bounds
            .as_ref()
            .and_then(|bounds| bounds.get(axis))
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(f64::MAX / 4.0)
    };
    coordinate("y") * 10_000.0 + coordinate("x")
}
