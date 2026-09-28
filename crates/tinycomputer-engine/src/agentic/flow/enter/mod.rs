//! The slot matcher behind `enter`: which field takes which text.
//!
//! All slots are matched in one request, one Choice per slot over the same
//! editable fields, and assigned greedily by probability so two slots can never
//! claim one field. Each text is then delivered with read-back verification
//! (`deliver_text`), top to bottom in screen order. A slot with no visible
//! field first runs a short `do` loop to reveal one.
//!
//! `assign` matches slots to fields and asks which the form wants; `fill`
//! delivers each text and verifies it.

mod assign;
mod fill;

use std::collections::BTreeSet;

use tinycomputer_bus::{FlowLoop, Slot, StepOutcome};

use super::{
    AgentBackend, Ended, FlowRun, Halt, StepLog,
    validate::{references, substitute, substitute_safe},
    view::{Candidate, Screen},
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
/// Details no picker offered, on a screen with no editable field at all,
/// after which the rest are not looked for one by one: this is not the form
/// they go in, and the step fails for a rescue to read rather than grounding
/// a picker per detail.
const BLIND_PICK_MISSES: usize = 1;

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
