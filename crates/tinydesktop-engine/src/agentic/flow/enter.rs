//! The slot matcher behind `enter`: which field takes which text.
//!
//! All slots are matched in one request, one Choice per slot over the same
//! editable fields, and assigned greedily by probability so two slots can never
//! claim one field. Each text is then delivered with read-back verification
//! (`deliver_text`), top to bottom in screen order. A slot with no visible
//! field first runs a short `do` loop to reveal one.

use std::collections::BTreeSet;

use serde_json::Value;
use tinydesktop_bus::{FlowLoop, Slot, StepOutcome};

use super::{
    AgentBackend, Ended, FlowRun, Halt, StepLog,
    ask::{self, CAP, Questions, chosen, elements, numbered},
    backend::deliver_text,
    memory::{learn, recall, remember},
    validate::{substitute, substitute_safe},
    view::{Candidate, Screen, label, signature},
};

/// Least probability a slot assignment needs.
const SLOT_FLOOR: f64 = 0.4;
/// Turns spent revealing fields that are not on screen yet.
const REVEAL_TURNS: u32 = 4;

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
        let mut revealed = false;
        for _ in 0..3 {
            if pending.is_empty() {
                break;
            }
            let mut screen = self.look().await?;
            if editable(&screen).len() < pending.len() && !screen.unexplored.is_empty() {
                self.explore(&mut screen).await;
            }
            let fields = editable(&screen);
            if fields.is_empty() {
                if revealed {
                    break;
                }
                revealed = true;
                let names = pending
                    .iter()
                    .map(|index| slots[*index].slot.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                self.accomplish(
                    log,
                    &format!("show the editable fields for: {names}"),
                    REVEAL_TURNS,
                )
                .await?;
                continue;
            }
            let assignments = self.assign(log, &screen, &slots, &pending, &fields).await?;
            if assignments.is_empty() {
                if revealed {
                    break;
                }
                revealed = true;
                let names = pending
                    .iter()
                    .map(|index| slots[*index].slot.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                self.accomplish(log, &format!("show the fields for: {names}"), REVEAL_TURNS)
                    .await?;
                continue;
            }
            for assignment in assignments {
                let slot = &slots[assignment.slot];
                if self.fill(log, slot, &assignment.field).await? {
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
        if pending.is_empty() {
            Ok(Ended::new(
                StepOutcome::Done,
                format!("entered {} value(s)", slots.len()),
            ))
        } else {
            Err(Halt::Failed(format!(
                "no field was found for: {}",
                pending
                    .iter()
                    .map(|index| slots[*index].slot.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )))
        }
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

    /// Delivers one slot's text and reports whether it verifiably arrived.
    async fn fill(
        &mut self,
        log: &mut StepLog,
        slot: &Slot,
        field: &Candidate,
    ) -> Result<bool, Halt> {
        let app = self.app.clone();
        let target = field.clone();
        let text = slot.text.clone();
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
