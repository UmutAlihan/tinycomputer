//! The `choose` step: picking an option in a list, an autocomplete box, or a
//! date picker, and putting back text the attempt changed.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use tinycomputer_bus::{
    ChooseStep, FlowAction, FlowLoop, FlowStopReason, IfStep, JevOperation, PickStep, ReadStep,
    RepeatStep, StepOutcome,
};
use tinycomputer_core::surface::{Group, result_families};
use tinycomputer_core::{Criterion, Record, rank};

use crate::workspace::BROWSER;

use crate::agentic::flow::{
    Ended, FlowRun, Halt, StepLog,
    act::{DONE, SCREEN_VIEW},
    ask::{self, Questions, chosen, condition, numbered},
    backend::{AgentBackend, deliver_text},
    escalate::Belief,
    ground::Grounded,
    memory::{learn, remember},
    validate::{MAX_REPEAT, substitute_safe},
    view::{Candidate, Screen, element_kind, is_destructive, label, target_payload},
};

use super::*;

impl<B: AgentBackend + Sync> FlowRun<'_, B> {

    async fn choose(&mut self, log: &mut StepLog, choose: &ChooseStep) -> Result<Ended, Halt> {
        // `what` and `option` are shown to Jev, so a secret is never
        // expanded into them; validation already rejects one there.
        let what = substitute_safe(&choose.what, &self.vars, &self.facts);
        let option = substitute_safe(&choose.option, &self.vars, &self.facts);
        self.pick_option(log, &what, &option, false, true).await
    }

    /// Picks `option` in `what` as a person works a list, an autocomplete
    /// box, or a date picker: take the option if it shows, else open the
    /// control, page a calendar forward to a date, or type the option to
    /// filter it. Only an element that shows the option is ever pressed, so
    /// a list that never shows it fails the step rather than picking another.
    ///
    /// A `private` option — a secret `enter` could not type into a field — is
    /// never written into a question: only elements that already show it are
    /// offered, so Jev sees nothing the page does not.
    ///
    /// `into_focus` lets it type the option wherever the focus is, as an
    /// opened autocomplete expects. `enter` turns that off for a slot with
    /// no field: the focus there is the field it just filled for another
    /// slot, and typing into it would spoil that value.
    ///
    /// A deliberating run records every text field's value first, and puts
    /// back any the attempts changed when the step fails: typing the option
    /// to filter a list lands wherever the focus is, and on a form with no
    /// such list that is the field filled last (live on Emirates, `Female`
    /// for a gender the form never asked for turned `Raina` into
    /// `RainaFemale`).
    pub(super) async fn pick_option(
        &mut self,
        log: &mut StepLog,
        what: &str,
        option: &str,
        private: bool,
        into_focus: bool,
    ) -> Result<Ended, Halt> {
        let before = if self.deliberates(FlowLoop::Checkpoint) {
            Some(held_text(&self.look().await?))
        } else {
            None
        };
        let result = self
            .try_option(log, what, option, private, into_focus)
            .await;
        if let (Some(before), Err(Halt::Failed(_))) = (before, &result) {
            self.restore_text(log, &before).await?;
        }
        result
    }

    /// Puts back every field of `before` whose text the step changed.
    async fn restore_text(
        &mut self,
        log: &mut StepLog,
        before: &[(Candidate, String)],
    ) -> Result<(), Halt> {
        let screen = self.look().await?;
        let alike = |screen_candidates: &[Candidate], kind: &str| {
            screen_candidates
                .iter()
                .filter(|candidate| element_kind(candidate) == kind)
                .count()
        };
        let before_candidates = before
            .iter()
            .map(|(field, _)| field.clone())
            .collect::<Vec<_>>();
        for (field, text) in before {
            let kind = element_kind(field);
            // Only a field told apart by its kind alone is put back: rows of
            // a list share one, and a field that refused text takes none.
            if self.refused.contains(&kind)
                || alike(&before_candidates, &kind) != 1
                || alike(&screen.candidates, &kind) != 1
            {
                continue;
            }
            let Some(now) = screen
                .candidates
                .iter()
                .find(|candidate| element_kind(candidate) == kind)
                .cloned()
            else {
                continue;
            };
            let current = now
                .value
                .as_ref()
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned();
            if current == text.trim() {
                continue;
            }
            log.used(FlowLoop::Checkpoint);
            let app = self.app.clone();
            let target = now.clone();
            let previous = text.clone();
            let reply = self
                .act(log, "retype (undo)", Some(&now), move |backend| {
                    deliver_text(&backend, &app, &target, &previous)
                })
                .await?;
            self.history.push(format!(
                "the step typed into {} by mistake; put its text back, ok={}",
                label(&now),
                reply.ok
            ));
            self.runtime.journal.record("restore", || {
                json!({
                    "step": self.step,
                    "rungs": ["retype"],
                    "restored": reply.ok,
                    "target": label(&now),
                })
            });
        }
        Ok(())
    }

    async fn try_option(
        &mut self,
        log: &mut StepLog,
        what: &str,
        option: &str,
        private: bool,
        into_focus: bool,
    ) -> Result<Ended, Halt> {
        let purpose = if private {
            format!("pick the option in {what} that shows the value being entered")
        } else {
            format!("pick the option {option:?} in {what}")
        };
        for attempt in 0..4 {
            let screen = self.look().await?;
            if !private && let Some(ended) = self.made_already(&screen, what, option, attempt) {
                return Ok(ended);
            }
            let pool = clickable(&screen.candidates)
                .into_iter()
                .filter(|candidate| !is_destructive(candidate, &screen, &self.stop_before))
                .collect::<Vec<_>>();
            // A field that holds the typed option is where it was typed,
            // not one of the options it offers.
            let pool = closest(
                pool.into_iter()
                    .filter(|candidate| {
                        mentions(candidate, option)
                            && !editable(candidate)
                            && !lists_more_than(candidate, option)
                    })
                    .collect(),
            );
            let pool = within(pool, what);
            // Matches that all name one option leave nothing to judge; a
            // private option is never judged, since Jev is not told it. An
            // option no control names is a description ("the lowest fare"),
            // matched by Jev among the page's option controls.
            let grounded = if pool.is_empty() && !private {
                self.described(log, &screen, what, option).await?
            } else if private || one_option(&pool) {
                plainest(pool).map(|candidate| Grounded {
                    candidate,
                    confidence: 1.0,
                })
            } else {
                self.ground(log, &screen, &purpose, &purpose, pool).await?
            };
            if let Some(grounded) = &grounded
                && is_checked(&grounded.candidate)
            {
                self.history
                    .push(format!("{} is already chosen", label(&grounded.candidate)));
                self.remember_choice(&format!("chose {option:?} in {what}"));
                return Ok(Ended::new(
                    StepOutcome::AlreadyDone,
                    format!("{option:?} was already chosen"),
                ));
            }
            if let Some(grounded) = grounded {
                log.confidence = Some(grounded.confidence);
                let target = grounded.candidate;
                let clicked = target.clone();
                // A private option was matched because it already shows the
                // value being entered; logging its label would write that
                // value into history and the step's action record, exactly
                // what picking privately is meant to avoid. The redacted
                // copy still carries the real ref and role, so the click
                // itself is unaffected.
                let logged = if private {
                    redacted(&target)
                } else {
                    target.clone()
                };
                let reply = self
                    .act(log, "click", Some(&logged), move |backend| {
                        backend.execute(JevOperation::Click, Some(clicked), None)
                    })
                    .await?;
                if reply.ok {
                    learn(&mut self.learned, remember(&self.app, &purpose, &target));
                    self.history.push(if private {
                        format!("chose the value shown in {what}")
                    } else {
                        format!("chose an option with {}", label(&target))
                    });
                    self.remember_choice(&if private {
                        format!("chose the value in {what}")
                    } else {
                        format!("chose {option:?} in {what}")
                    });
                    return Ok(Ended::new(
                        StepOutcome::Done,
                        if private {
                            format!("chose the value in {what}")
                        } else {
                            format!("chose {option:?}")
                        },
                    ));
                }
            }
            self.another_way(log, attempt, &screen, what, option, into_focus)
                .await?;
        }
        Err(Halt::Failed(if private {
            format!("the value was not found in {what}")
        } else {
            format!("{option:?} was not found in {what}")
        }))
    }

    /// `AlreadyDone` when `screen` shows `option` chosen already: a checked
    /// option control, or — before this step acts, since what it types to
    /// filter a list would read back as the choice — a field holding it.
    fn made_already(
        &mut self,
        screen: &Screen,
        what: &str,
        option: &str,
        attempt: usize,
    ) -> Option<Ended> {
        let shown = already_chosen(screen, option)
            .map(|chosen| format!("{} is already chosen", label(&chosen)))
            .or_else(|| {
                (attempt == 0)
                    .then(|| already_holds(screen, option, &self.typed))
                    .flatten()
                    .map(|holder| format!("{} already shows {option:?}", label(&holder)))
            })?;
        self.history.push(shown);
        self.remember_choice(&format!("chose {option:?} in {what}"));
        Some(Ended::new(
            StepOutcome::AlreadyDone,
            format!("{option:?} was already chosen"),
        ))
    }

    /// The option control on `screen` that fits `option` read as a
    /// description, by Jev; `None` when no control fits well enough.
    async fn described(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        what: &str,
        option: &str,
    ) -> Result<Option<Grounded>, Halt> {
        let options = clickable(&screen.candidates)
            .into_iter()
            .filter(|candidate| {
                is_one_option(candidate) && !is_destructive(candidate, screen, &self.stop_before)
            })
            .collect::<Vec<_>>();
        if options.is_empty() {
            return Ok(None);
        }
        let purpose = format!("pick the option in {what} that fits: {option}");
        Ok(self
            .ground(log, screen, &purpose, &purpose, options)
            .await?
            .filter(|grounded| grounded.confidence >= LOCATE_FLOOR))
    }

}
