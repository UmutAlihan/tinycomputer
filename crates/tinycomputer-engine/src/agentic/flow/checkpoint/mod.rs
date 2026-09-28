//! Checkpoints: where an action started, and undoing it back there.
//!
//! Before a deliberating `do` move presses an element, the screen and the
//! surface's address are recorded as a [`Checkpoint`], and the action is
//! classified by how it can be undone ([`Reversibility`]). When the action
//! turns out to be a mistake, the cheapest undo that fits it is tried:
//!
//! | The action…             | Undone by                                    |
//! |-------------------------|----------------------------------------------|
//! | left the page           | going back, then loading the recorded address |
//! | flipped a toggle        | pressing it again                            |
//! | anything else           | Escape                                       |
//!
//! Every undo is then **verified**: the screen must match the checkpoint
//! again, [`RESTORED`] of its elements back, on the recorded address. An
//! undo that claimed to restore — going back, pressing again — and did not
//! fails the step closed rather than acting on a screen nobody understands;
//! Escape is best effort, and a miss is only noted.

use std::collections::BTreeSet;

use serde_json::json;
use tinycomputer_bus::{FlowLoop, JevOperation};

use super::{
    AgentBackend, FlowRun, Halt, StepLog,
    expect::{self, Effect},
    view::{Candidate, Screen, is_destructive, label, signature},
};

/// Least share of a checkpoint's elements a screen must show again to count
/// as restored.
pub(super) const RESTORED: f64 = 0.8;

/// How an action can be undone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reversibility {
    /// Escape undoes it: it opened or scrolled something.
    Reversible,
    /// It can be put back: a toggle pressed again, a page gone back from.
    Restorable,
    /// Nothing undoes it: it sends, pays, deletes, or submits.
    Irreversible,
}

/// How `effect` of pressing `target` on `screen` can be undone.
pub(super) fn classify(
    effect: &Effect,
    target: &Candidate,
    screen: &Screen,
    stop_before: &[String],
) -> Reversibility {
    if is_destructive(target, screen, stop_before) {
        return Reversibility::Irreversible;
    }
    match effect {
        Effect::Toggles(_) | Effect::Navigates => Reversibility::Restorable,
        _ => Reversibility::Reversible,
    }
}

/// What a screen looked like, and where, before an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Checkpoint {
    /// The ref-free identities of what was on screen, and its text.
    marks: BTreeSet<String>,
    /// The surface's address, when it has one.
    pub(super) location: Option<String>,
}

impl Checkpoint {
    /// A checkpoint of `screen` at `location`.
    pub(super) fn of(screen: &Screen, location: Option<&str>) -> Self {
        Self {
            marks: marks(screen),
            location: location.map(str::to_owned),
        }
    }

    /// The share of this checkpoint's marks `screen` shows; 1 for an empty
    /// checkpoint, which nothing can contradict.
    pub(super) fn similarity(&self, screen: &Screen) -> f64 {
        if self.marks.is_empty() {
            return 1.0;
        }
        let now = marks(screen);
        let kept = self.marks.intersection(&now).count();
        let count = |value: usize| f64::from(u32::try_from(value).unwrap_or(u32::MAX));
        count(kept) / count(self.marks.len())
    }

    /// Whether `screen`, at `location`, is this checkpoint again.
    pub(super) fn restored(&self, screen: &Screen, location: Option<&str>) -> bool {
        let here = self.location.is_none() || self.location.as_deref() == location;
        here && self.similarity(screen) >= RESTORED
    }
}

fn marks(screen: &Screen) -> BTreeSet<String> {
    screen
        .candidates
        .iter()
        .map(signature)
        .chain(screen.context.iter().map(|line| format!("text:{line}")))
        .collect()
}

/// How an undo went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Restore {
    /// The rungs tried, in order: `back`, `navigate`, `press again`,
    /// `escape`.
    pub(super) rungs: Vec<&'static str>,
    /// Whether the screen matches the checkpoint again.
    pub(super) restored: bool,
    /// Whether a rung that claims to restore was used, so a miss must stop
    /// the step.
    pub(super) claimed: bool,
}

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Journals a checkpoint taken before `target` is pressed.
    pub(super) fn checkpointed(
        &self,
        log: &mut StepLog,
        target: &Candidate,
        reversibility: Reversibility,
    ) {
        log.used(FlowLoop::Checkpoint);
        self.runtime.journal.record("checkpoint", || {
            json!({
                "step": self.step,
                "target": label(target),
                "reversibility": format!("{reversibility:?}").to_ascii_lowercase(),
                "location": self.location,
            })
        });
    }

    /// Undoes the action that pressed `target` with `effect`, back to
    /// `checkpoint`, and checks the screen shows it again.
    ///
    /// # Errors
    ///
    /// [`Halt::Failed`] when a restoring rung ran and the screen still does
    /// not match the checkpoint: acting on from an unknown screen would
    /// compound the mistake.
    pub(super) async fn restore(
        &mut self,
        log: &mut StepLog,
        checkpoint: &Checkpoint,
        target: Option<&Candidate>,
        effect: Option<&Effect>,
    ) -> Result<Restore, Halt> {
        log.used(FlowLoop::Checkpoint);
        let mut rungs = Vec::new();
        let app = self.app.clone();
        let wandered = checkpoint.location.is_some() && self.location != checkpoint.location;
        if wandered {
            rungs.push("back");
            let from = app.clone();
            self.act(log, "back (undo)", None, move |backend| backend.back(&from))
                .await?;
            if self.location != checkpoint.location
                && let Some(url) = checkpoint.location.clone()
            {
                rungs.push("navigate");
                self.act(log, "navigate (undo)", None, move |backend| {
                    backend.navigate(&url)
                })
                .await?;
            }
        } else if let (Some(target), Some(effect)) = (target, effect)
            && effect.toggles()
        {
            let screen = self.look().await?;
            if let Some(again) = expect::find(target, &screen).cloned() {
                rungs.push("press again");
                let pressed = again.clone();
                self.act(log, "click (undo)", Some(&again), move |backend| {
                    backend.execute(JevOperation::Click, Some(pressed), None)
                })
                .await?;
            }
        }
        if rungs.is_empty() {
            rungs.push("escape");
            self.act(log, "press escape (undo)", None, move |backend| {
                backend.press(&app, "escape")
            })
            .await?;
        }
        let screen = self.look().await?;
        let similarity = checkpoint.similarity(&screen);
        let restored = checkpoint.restored(&screen, self.location.as_deref());
        let claimed = rungs.iter().any(|rung| *rung != "escape");
        self.runtime.journal.record("restore", || {
            json!({
                "step": self.step,
                "rungs": rungs,
                "restored": restored,
                "similarity": similarity,
            })
        });
        if claimed && !restored {
            return Err(Halt::Failed(format!(
                "undid a mistake ({}) but the screen does not match where it started ({:.0}% of it is back)",
                rungs.join(", then "),
                similarity * 100.0
            )));
        }
        Ok(Restore {
            rungs,
            restored,
            claimed,
        })
    }
}

#[cfg(test)]
mod test;
