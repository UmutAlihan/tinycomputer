//! Recovering from a turn that went wrong: undoing a regression or a
//! mistaken press, backtracking into the next-best candidate, and noticing
//! a screen that oscillates.

use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

use serde_json::json;
use tinycomputer_bus::{FlowLoop, JevOperation, StepOutcome};
use tinyinference_decisions::{Answer, EvaluationRequest};

use crate::agentic::flow::{
    Ended, FlowRun, Halt, StepLog,
    ask::{self, Questions, chosen, completion, level, obstacle, probability, progress},
    attention::Cleared,
    backend::AgentBackend,
    checkpoint::{Checkpoint, Reversibility, classify},
    denoise,
    escalate::Belief,
    expect::{self, Effect, Outcome},
    ground::{AGREED, Grounded, Opening},
    memory::{learn, remember},
    view::{
        Candidate, Screen, change_note, element_kind, fingerprint, is_destructive, label, signature,
    },
    wide::{Dismissal, Prepared},
};

use super::*;

impl<B: AgentBackend + Sync> FlowRun<'_, B> {

    /// Dismisses an obstacle or undoes a regression; `true` when it acted.
    async fn recover(
        &mut self,
        log: &mut StepLog,
        state: &mut DoState,
        screen: &Screen,
        intent: &str,
        judged: &Judgement,
    ) -> Result<bool, Halt> {
        if judged.blocked.unwrap_or_default() >= BLOCKED && state.obstacles < MAX_OBSTACLES {
            state.obstacles += 1;
            log.used(FlowLoop::Obstacles);
            match &judged.dismissal {
                Some(dismissal) => self.dismiss(log, dismissal.clone()).await?,
                None => self.clear_obstacle(log, screen, intent).await?,
            }
            state.last = None;
            // Clearing an obstacle is not a press to weigh an oscillation
            // against: the screens before and after belong to two different
            // situations, so a later coincidence between them is not the
            // two presses that undid each other.
            state.pressed_before = None;
            state.seen.clear();
            return Ok(true);
        }
        let regressed = match (&state.last, judged.progress) {
            (Some(previous), Some(now)) => previous
                .progress
                .filter(|before| before - now >= REGRESSION)
                .map(|before| {
                    (
                        format!("progress {before:.2} -> {now:.2}"),
                        previous.target.clone(),
                    )
                }),
            _ => None,
        };
        let unhelpful = state
            .last
            .as_ref()
            .filter(|previous| previous.target.is_some())
            .zip(judged.helped.filter(|helped| *helped < UNHELPFUL))
            .map(|(previous, helped)| {
                (
                    format!("it did not help (confidence {helped:.2})"),
                    previous.target.clone(),
                )
            });
        let mistaken = state
            .last
            .as_ref()
            .filter(|previous| previous.target.is_some())
            .zip(judged.intended)
            .filter(|(previous, intended)| {
                *intended < CLEAR_MISTAKE
                    || (*intended < MISTAKE && matches!(previous.outcome, Some(Outcome::Missed(_))))
            })
            .map(|(previous, intended)| {
                let seen = match &previous.outcome {
                    Some(Outcome::Missed(why)) => format!("{why}; "),
                    _ => String::new(),
                };
                (
                    format!("{seen}it did not do what it was meant to (confidence {intended:.2})"),
                    previous.target.clone(),
                )
            });
        let Some((why, target)) = mistaken.or(regressed).or(unhelpful) else {
            return Ok(false);
        };
        if !self.enabled(FlowLoop::Undo) || state.undos >= MAX_UNDOS {
            return Ok(false);
        }
        state.undos += 1;
        log.used(FlowLoop::Undo);
        if let Some(target) = &target {
            state.banned.insert(signature(target));
            self.ledger.tried(format!(
                "pressed {}: it made things worse ({why})",
                label(target)
            ));
        }
        self.undo(log, state, &why, target.as_ref()).await?;
        self.plan_branch(state);
        state.last = None;
        // A verified undo is supposed to return to an earlier screen, so
        // seeing it again next turn is not an oscillation: without this,
        // `note_oscillation` would see the pre-mistake screen twice with
        // the mistaken press in between and ban `pressed_before` — the
        // correct earlier press that undoing the mistake deliberately
        // brought back.
        state.pressed_before = None;
        state.seen.clear();
        Ok(true)
    }

    /// Undoes the last press, which `why` says was a mistake: back to its
    /// checkpoint, verified, under deliberation; with Escape otherwise.
    async fn undo(
        &mut self,
        log: &mut StepLog,
        state: &DoState,
        why: &str,
        target: Option<&Candidate>,
    ) -> Result<(), Halt> {
        let expected = state
            .last
            .as_ref()
            .and_then(|last| last.expected.clone())
            .filter(|_| self.deliberates(FlowLoop::Checkpoint));
        if let Some(expected) = expected {
            let restore = self
                .restore(log, &expected.checkpoint, target, Some(&expected.effect))
                .await?;
            self.history.push(format!(
                "that was a mistake ({why}); undid it ({}){}",
                restore.rungs.join(", then "),
                if restore.restored {
                    " and the screen is back where it was"
                } else {
                    ", though the screen is not quite back where it was"
                }
            ));
            return Ok(());
        }
        let app = self.app.clone();
        self.act(log, "press escape (undo)", None, move |backend| {
            backend.press(&app, "escape")
        })
        .await?;
        self.history.push(format!(
            "that made things worse ({why}); undid it and will try something else"
        ));
        Ok(())
    }

    /// After an undo, the candidate to try next: the best runner-up of the
    /// grounding that chose the mistake, not banned, while the step has
    /// branches left.
    fn plan_branch(&self, state: &mut DoState) {
        if !self.deliberates(FlowLoop::Backtrack) {
            return;
        }
        let limit = if self.deep() {
            MAX_BRANCHES.0
        } else {
            MAX_BRANCHES.1
        };
        if state.branches >= limit {
            return;
        }
        state.branch = self
            .frontier
            .iter()
            .find(|candidate| !state.banned.contains(&signature(candidate)))
            .cloned();
    }

    /// Checks the last press's expected effect against `screen`.
    fn check_expectation(&self, log: &mut StepLog, state: &mut DoState, screen: &Screen) {
        let Some(last) = state.last.as_mut() else {
            return;
        };
        let (Some(target), Some(expected)) = (&last.target, &last.expected) else {
            return;
        };
        log.used(FlowLoop::Expectation);
        let moved =
            expected.checkpoint.location.is_some() && expected.checkpoint.location != self.location;
        let outcome = expect::check(&expected.effect, target, &last.before, screen, moved);
        self.runtime.journal.record("expect", || {
            json!({
                "step": self.step,
                "target": label(target),
                "effect": format!("{:?}", expected.effect),
                "outcome": match &outcome {
                    Outcome::Met => "met".to_owned(),
                    Outcome::Missed(why) => format!("missed: {why}"),
                    Outcome::Unclear => "unclear".to_owned(),
                },
            })
        });
        last.outcome = Some(outcome);
    }

    /// Bans both presses that took the screen back to where it was two
    /// turns ago: pressed in turn, they undo each other.
    fn note_oscillation(&mut self, log: &mut StepLog, state: &mut DoState, screen: &Screen) {
        if !self.deliberates(FlowLoop::Denoise) {
            return;
        }
        let now = fingerprint(screen);
        if denoise::oscillates(&state.seen, &now) {
            let pair = [
                state.last.as_ref().and_then(|last| last.target.clone()),
                state.pressed_before.clone(),
            ];
            let banned = pair
                .iter()
                .flatten()
                .map(|target| {
                    state.banned.insert(signature(target));
                    label(target)
                })
                .collect::<Vec<_>>();
            if !banned.is_empty() {
                log.used(FlowLoop::Denoise);
                self.ledger.tried(format!(
                    "pressed {}: the screen went back and forth",
                    banned.join(" and ")
                ));
                self.history.push(format!(
                    "the screen returned to where it was two turns ago; not pressing {} again",
                    banned.join(" or ")
                ));
                self.runtime.journal.record(
                    "denoise",
                    || json!({"step": self.step, "oscillation": banned}),
                );
            }
        }
        state.seen.push(now);
    }

    /// What pressing `target` with `operation` on `screen` should change,
    /// and the checkpoint it can be undone back to, when the run
    /// deliberates on effects.
    fn expect(
        &self,
        log: &mut StepLog,
        operation: &str,
        target: &Candidate,
        screen: &Screen,
    ) -> Option<Expected> {
        if !self.deliberates(FlowLoop::Expectation) {
            return None;
        }
        let effect = expect::predict(operation, target);
        let checkpoint = Checkpoint::of(screen, self.location.as_deref());
        let reversibility = classify(&effect, target, screen, &self.stop_before);
        if reversibility == Reversibility::Restorable && self.deliberates(FlowLoop::Checkpoint) {
            self.checkpointed(log, target, reversibility);
        }
        Some(Expected { effect, checkpoint })
    }

    /// The backtrack's `branch`, when it is still on `screen` and one yes/no
    /// question confirms it serves `purpose`.
    async fn try_branch(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        purpose: &str,
        branch: Candidate,
    ) -> Result<Option<Grounded>, Halt> {
        let Some(present) = screen
            .candidates
            .iter()
            .find(|candidate| signature(candidate) == signature(&branch))
            .cloned()
        else {
            return Ok(None);
        };
        log.used(FlowLoop::Backtrack);
        let answers = self
            .ask(
                log,
                ask::request(
                    self.model(),
                    self.state(screen, purpose),
                    Questions::default().with(
                        "confirm",
                        ask::corroborate(purpose, &present, self.include_values),
                    ),
                ),
            )
            .await?;
        let confirmed = probability(&answers, "confirm").unwrap_or_default();
        let accepted = confirmed >= AGREED;
        self.runtime.journal.record("backtrack", || {
            json!({
                "step": self.step,
                "candidate": label(&present),
                "confirmed": confirmed,
                "accepted": accepted,
            })
        });
        Ok(accepted.then(|| {
            self.history.push(format!(
                "backtracking: trying the next-best candidate, {}",
                label(&present)
            ));
            Grounded {
                candidate: present,
                confidence: confirmed,
            }
        }))
    }

}
