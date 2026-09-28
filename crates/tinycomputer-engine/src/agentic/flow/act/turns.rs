//! The turn loop of a `do` step: judge the screen, recover from a bad
//! turn, make a move, and note what changed.

use std::time::Instant;

use serde_json::json;
use tinycomputer_bus::StepOutcome;

use crate::agentic::flow::{
    Ended, FlowRun, Halt, StepLog,
    backend::AgentBackend,
    view::{Screen, change_note, fingerprint, label, signature},
};

use super::{
    DONE, DoState, LastAction, MAX_IDLE_WAITS, Move, STALL_TURNS, closed_the_overlay, creates_new,
    finish_floor, finished,
};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Runs the `do` loop for `intent` for at most `max_turns` turns.
    pub(in crate::agentic::flow) async fn accomplish(
        &mut self,
        log: &mut StepLog,
        intent: &str,
        max_turns: u32,
    ) -> Result<Ended, Halt> {
        let mut state = DoState::default();
        let ended = self.turns(log, &mut state, intent, max_turns).await;
        self.end_turn(&mut state);
        ended
    }

    /// Journals the turn under way, if any: how many decisions it took and
    /// how long it ran.
    fn end_turn(&self, state: &mut DoState) {
        let Some((turn, started, before, rounds_before)) = state.turn.take() else {
            return;
        };
        self.runtime.journal.record("turn", || {
            json!({
                "step": self.step,
                "turn": turn,
                "decisions": self.decisions.saturating_sub(before),
                "rounds": self.rounds.saturating_sub(rounds_before),
                "wall_ms": crate::agentic::journal::millis(started.elapsed()),
            })
        });
    }

    async fn turns(
        &mut self,
        log: &mut StepLog,
        state: &mut DoState,
        intent: &str,
        max_turns: u32,
    ) -> Result<Ended, Halt> {
        for turn in 0..max_turns {
            self.end_turn(state);
            state.turn = Some((turn, Instant::now(), self.decisions, self.rounds));
            log.turns = log.turns.saturating_add(1);
            let screen = self.look().await?;
            self.note_change(state, &screen)?;
            self.note_oscillation(log, state, &screen);
            if state.first.is_none() {
                state.first = Some(screen.clone());
            }
            if let Some(ended) = state
                .last
                .as_ref()
                .and_then(|last| closed_the_overlay(last, &screen, intent))
            {
                return Ok(ended);
            }
            // The root of the turn's tree: what needs attention first. A
            // distraction cleared means a fresh look before judging.
            if self
                .attend(log, &screen, intent, &mut state.cleared)
                .await?
            {
                state.last = None;
                continue;
            }
            self.check_expectation(log, state, &screen);
            let judged = self.judge_turn(log, state, &screen, intent).await?;
            let mut judged = self
                .settle_done(log, state, &screen, intent, judged, turn)
                .await?;
            if judged.next == "finished"
                && judged.done.is_some_and(|done| done < finish_floor(turn))
            {
                // The move chooser's "finished" is one vote. Before anything
                // is done it needs the completion judge's full bar, since
                // skipping a step derails the rest; after acting it stands
                // unless the judge leans the other way.
                self.history.push(
                    "the screen does not yet clearly show this step done; act on it".to_owned(),
                );
                "activate".clone_into(&mut judged.next);
            }
            let creating = creates_new(intent) && log.actions.is_empty();
            if !creating && let Some(ended) = finished(log, &judged, turn) {
                return Ok(ended);
            }
            if creating && judged.next == "finished" {
                self.history.push(
                    "this step creates something new, so something already on screen cannot count; act first"
                        .to_owned(),
                );
            }
            if self.recover(log, state, &screen, intent, &judged).await? {
                continue;
            }
            if judged.next == "wait" && state.idle_waits >= MAX_IDLE_WAITS {
                self.history.push(
                    "did not wait again: the page has settled, so judge it as it is or act on it"
                        .to_owned(),
                );
                continue;
            }
            let branch = state.branch.take();
            match self
                .make_move(log, &screen, intent, &judged, &state.banned, branch)
                .await?
            {
                Move::Ended(ended) => return Ok(ended),
                Move::Acted(target, expected) => {
                    state.pressed_before = state.last.as_ref().and_then(|last| last.target.clone());
                    state.last = Some(LastAction {
                        target: target.map(|target| *target),
                        before: screen,
                        progress: judged.progress,
                        waited: judged.next == "wait",
                        expected: expected.map(|expected| *expected),
                        outcome: None,
                    });
                }
                Move::Skipped => {}
            }
        }
        let screen = self.look().await?;
        // A dismissal on the very last permitted turn leaves no other
        // evidence once the loop stops: the overlay is gone, but the
        // completion judge sees only the screen after the fact. Apply the
        // same check here that runs at the top of every earlier turn, or a
        // dismissal that succeeded on the last turn is reported as failed.
        if let Some(ended) = state
            .last
            .as_ref()
            .and_then(|last| closed_the_overlay(last, &screen, intent))
        {
            return Ok(ended);
        }
        let judged = self.judge(log, &screen, intent, None).await?;
        let judged = self
            .settle_done(log, state, &screen, intent, judged, max_turns)
            .await?;
        if judged.done.unwrap_or_default() >= DONE {
            return Ok(Ended::new(
                StepOutcome::Done,
                "accomplished on the last turn",
            ));
        }
        Err(Halt::Failed(format!(
            "not accomplished after {max_turns} turns"
        )))
    }

    /// Records what the last action changed, banning an element that changed
    /// nothing and failing the step after [`STALL_TURNS`] such turns.
    ///
    /// A wait that changes nothing is not a stall: the page has settled, and
    /// Jev is told so. It is not let wait again after [`MAX_IDLE_WAITS`] of
    /// them, which leaves it to judge or act on the page as it stands.
    fn note_change(&mut self, state: &mut DoState, screen: &Screen) -> Result<(), Halt> {
        let Some(previous) = &state.last else {
            return Ok(());
        };
        let changed = fingerprint(&previous.before) != fingerprint(screen);
        let note = change_note(&previous.before, screen, changed);
        if changed {
            state.unchanged = 0;
            state.idle_waits = 0;
        } else if previous.waited {
            state.idle_waits = state.idle_waits.saturating_add(1);
            self.history.push(
                "waited: the page has finished loading and nothing changed, so waiting longer will not change it"
                    .to_owned(),
            );
            return Ok(());
        } else {
            state.unchanged = state.unchanged.saturating_add(1);
            if let Some(target) = &previous.target {
                state.banned.insert(signature(target));
                self.ledger.tried(format!(
                    "pressed {}: nothing on screen changed",
                    label(target)
                ));
            }
        }
        self.history.push(format!("after the last action: {note}"));
        if state.unchanged >= STALL_TURNS {
            return Err(Halt::Failed(
                "the last three actions changed nothing on screen".to_owned(),
            ));
        }
        Ok(())
    }
}
