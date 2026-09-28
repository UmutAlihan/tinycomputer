//! Making a `do` move: pressing a grounded control, a shortcut, a scroll,
//! or a wait, and clearing an obstacle out of the way.

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

use super::{DONE, ALREADY_DONE, LEANS_DONE, BLOCKED, REGRESSION, UNHELPFUL, SHORTCUT_FLOOR, STALL_TURNS, MAX_IDLE_WAITS, MAX_OBSTACLES, MAX_UNDOS, MISTAKE, CLEAR_MISTAKE, MAX_BRANCHES, SCREEN_VIEW, CHANGES_VIEW, MOVES, SHORTCUTS, LastAction, Expected, DoState, Move, creates_new, DISMISS_VERBS, OVERLAYS, words, closed_the_overlay, covered, threshold, finish_floor, finished, activate_purpose, judge::{Judgement, Speculated}};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {

    /// Carries out the move Jev chose.
    pub(super) async fn make_move(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        intent: &str,
        judged: &Judgement,
        banned: &BTreeSet<String>,
        branch: Option<Candidate>,
    ) -> Result<Move, Halt> {
        match judged.next.as_str() {
            "finished" if creates_new(intent) && log.actions.is_empty() => {
                // Jev sees an existing item and calls it done; make a new one
                // with the shortcut it would use, or by pressing a control.
                let next = Judgement {
                    next: if judged.shortcut.is_some() {
                        "shortcut".to_owned()
                    } else {
                        "activate".to_owned()
                    },
                    ..judged.clone()
                };
                Box::pin(self.make_move(log, screen, intent, &next, banned, branch)).await
            }
            "finished" => Ok(Move::Ended(Ended::new(
                StepOutcome::Done,
                "Jev chose finished",
            ))),
            "stuck" => Err(Halt::Failed(
                "no visible control or standard shortcut moves toward the step".to_owned(),
            )),
            "wait" => {
                self.act(log, "wait", None, |backend| {
                    backend.execute(JevOperation::Wait, None, None)
                })
                .await?;
                Ok(Move::Acted(None, None))
            }
            "shortcut" => {
                let Some((combo, name)) = judged.shortcut else {
                    self.history
                        .push("no standard shortcut fits; press a visible control".to_owned());
                    return Ok(Move::Skipped);
                };
                if combo == "return" && screen.surface != "window" {
                    self.history.push(format!(
                        "refused return while a {} is showing: it would press its default button",
                        screen.surface
                    ));
                    return Ok(Move::Skipped);
                }
                let app = self.app.clone();
                let reply = self
                    .act(log, &format!("press {combo}"), None, move |backend| {
                        backend.press(&app, combo)
                    })
                    .await?;
                self.history
                    .push(format!("pressed {combo} ({name}), ok={}", reply.ok));
                Ok(Move::Acted(None, None))
            }
            operation @ ("activate" | "expand" | "scroll") => {
                let pressed = self
                    .activate(log, screen, intent, operation, (banned, branch), judged)
                    .await?;
                Ok(match pressed {
                    Some((target, expected)) => {
                        Move::Acted(Some(Box::new(target)), expected.map(Box::new))
                    }
                    None => Move::Acted(None, None),
                })
            }
            other => {
                // A malformed or prompt-injected answer must fail closed
                // rather than default to a click: only the moves above are
                // ever offered to Jev.
                self.history.push(format!(
                    "ignored an unrecognized move {other:?}; only activate, shortcut, expand, scroll, wait, finished, and stuck are valid"
                ));
                Ok(Move::Skipped)
            }
        }
    }

    /// The elements a move of `capability` may target: not banned this
    /// step, and not of a kind that refused text.
    pub(super) fn pool(&self, screen: &Screen, capability: &str, banned: &BTreeSet<String>) -> Vec<Candidate> {
        screen
            .candidates
            .iter()
            .filter(|candidate| {
                candidate
                    .available_actions
                    .iter()
                    .any(|action| action == capability)
                    && !banned.contains(&signature(candidate))
                    && !self.refused.contains(&element_kind(candidate))
            })
            .cloned()
            .collect()
    }

    /// Grounds and performs an `activate`, `expand`, or `scroll` move; the
    /// element pressed, and under deliberation what the press expects.
    ///
    /// A `branch` left by a backtrack is tried first, confirmed with one
    /// yes/no question, before anything is grounded afresh.
    async fn activate(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        intent: &str,
        operation: &str,
        (banned, branch): (&BTreeSet<String>, Option<Candidate>),
        judged: &Judgement,
    ) -> Result<Option<(Candidate, Option<Expected>)>, Halt> {
        let (capability, jev_operation, verb) = match operation {
            "expand" => ("Expand", JevOperation::Expand, "expand"),
            "scroll" => ("Scroll", JevOperation::Scroll, "scroll"),
            _ => ("Click", JevOperation::Click, "click"),
        };
        let purpose = activate_purpose(verb, intent);
        let prepared = judged.prepared.get(operation);
        let speculated = judged.speculated.clone();
        let branched = match branch {
            Some(branch) if operation == "activate" => {
                self.try_branch(log, screen, &purpose, branch).await?
            }
            _ => None,
        };
        let grounded = match (branched, prepared, speculated) {
            (Some(branched), _, _) => Some(branched),
            (None, Some(prepared), _) => self.resolve(log, screen, &purpose, prepared).await?,
            (None, None, Some(speculated)) if operation == "activate" => {
                self.resume(log, screen, speculated.opening, Some(speculated.answers))
                    .await?
            }
            _ => {
                let pool = self.pool(screen, capability, banned);
                self.ground(log, screen, &purpose, intent, pool).await?
            }
        };
        let Some(grounded) = grounded else {
            self.history.push(format!(
                "no element clearly serves {verb} for this step; consider a shortcut or another move"
            ));
            return Ok(None);
        };
        let target = grounded.candidate;
        log.confidence = Some(grounded.confidence);
        if jev_operation == JevOperation::Click
            && is_destructive(&target, screen, &self.stop_before)
        {
            // Never call the backend, and never report this target through
            // `Move::Acted`: nothing happened, so it must not be banned as a
            // no-op or stalled toward `note_change`'s three-turn failure. The
            // step fails on this turn, with a note that says why.
            return Err(Halt::Failed(format!(
                "refused to press {} inside an ordinary step: it looks irreversible; a flow must use stop_before for that",
                label(&target)
            )));
        }
        let expected = self.expect(log, operation, &target, screen);
        let reply = self
            .press_uncovering(log, verb, &target, jev_operation)
            .await?;
        self.history
            .push(format!("{verb} {} ok={}", label(&target), reply.ok));
        if reply.ok {
            learn(&mut self.learned, remember(&self.app, intent, &target));
        }
        Ok(Some((target, expected)))
    }

    /// Performs `operation` on an already-vetted `target`. When the click
    /// is refused because something covers it — a drawer, a menu, or a
    /// result card's own click layer — presses Escape once and tries the
    /// same target again. Escape never chooses a new element.
    pub(in crate::agentic::flow) async fn press_uncovering(
        &mut self,
        log: &mut StepLog,
        verb: &str,
        target: &Candidate,
        operation: JevOperation,
    ) -> Result<tinycomputer_bus::DesktopResponse, Halt> {
        let chosen = target.clone();
        let reply = self
            .act(log, verb, Some(target), move |backend| {
                backend.execute(operation, Some(chosen), None)
            })
            .await?;
        if !covered(&reply) {
            return Ok(reply);
        }
        let app = self.app.clone();
        self.act(log, "press escape (uncover)", None, move |backend| {
            backend.press(&app, "escape")
        })
        .await?;
        self.history.push(format!(
            "{} was covered by something; pressed escape to close it",
            label(target)
        ));
        let retried = target.clone();
        self.act(log, verb, Some(target), move |backend| {
            backend.execute(operation, Some(retried), None)
        })
        .await
    }

    /// Dismisses whatever is blocking the step, choosing only safe controls.
    pub(super) async fn clear_obstacle(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        intent: &str,
    ) -> Result<(), Halt> {
        let pool = screen
            .candidates
            .iter()
            .filter(|candidate| {
                candidate
                    .available_actions
                    .iter()
                    .any(|action| action == "Click")
                    && !is_destructive(candidate, screen, &self.stop_before)
            })
            .take(ask::CAP)
            .cloned()
            .collect::<Vec<_>>();
        let keys = ask::numbered(pool.len());
        let mut options = keys
            .iter()
            .cloned()
            .zip(pool.iter().map(|node| crate::agentic::flow::view::describe(node, false)))
            .collect::<Vec<_>>();
        options.push(("escape".to_owned(), json!("Press Escape to close it.")));
        let answers = self
            .ask(
                log,
                ask::request(
                    self.model(),
                    self.state(screen, intent),
                    Questions::default().with(
                        "dismiss",
                        ask::options(
                            json!({
                                "task": "Something unrelated to the step is in the way. Choose how to close it without losing work and without doing anything irreversible.",
                                "step": intent,
                            }),
                            options,
                        ),
                    ),
                ),
            )
            .await?;
        let choice = chosen(&answers, "dismiss").map(|(choice, _)| choice);
        let target = choice
            .as_deref()
            .and_then(|choice| keys.iter().position(|key| key == choice))
            .and_then(|index| pool.get(index).cloned());
        if let Some(target) = target {
            let chosen_target = target.clone();
            self.act(log, "click (dismiss)", Some(&target), move |backend| {
                backend.execute(JevOperation::Click, Some(chosen_target), None)
            })
            .await?;
            self.history
                .push(format!("dismissed an obstacle with {}", label(&target)));
        } else {
            let app = self.app.clone();
            self.act(log, "press escape (dismiss)", None, move |backend| {
                backend.press(&app, "escape")
            })
            .await?;
            self.history
                .push("pressed escape to dismiss an obstacle".to_owned());
        }
        Ok(())
    }

}
