//! The run's working memory, as every wide-strategy question sees it.
//!
//! The narrow strategy shows Jev the last twenty history lines, a mix of
//! step outcomes, change notes, and runtime notes, and older steps fall out
//! of it. The ledger keeps what a person carrying out the task would keep in
//! mind instead:
//!
//! - every finished step, one line each, for the whole run;
//! - the most recent actions and what each changed, reaching back into the
//!   steps before this one: the click that left a page is often the only
//!   evidence that the page was dealt with (measured: a payment page judged
//!   "seat selection skipped" at 0.95 with the previous step's clicks in
//!   view, and at 0.48 without them);
//! - what was **tried and failed** in this step — a control that changed
//!   nothing, a move that made things worse, an overlay already dismissed —
//!   so it is not tried again;
//! - the step after this one, so "done" is judged against this step and not
//!   against something the next step will do;
//! - the names of the variables read so far, and the budget left.
//!
//! It never holds the goal-level brief: judged against the whole task, a
//! step looks unfinished (see `docs/specs/jev-briefing.md`). Every line is
//! built from masked text, and the request is masked again before it
//! leaves, so a secret's value never reaches Jev through it.

use serde_json::{Value, json};

/// Finished steps the ledger shows, newest last.
const MAX_FINISHED: usize = 40;
/// Recent history lines the ledger shows, newest last, across steps.
const MAX_RECENT: usize = 24;
/// Tried-and-failed notes kept for one step.
const MAX_TRIED: usize = 12;
/// Longest line the ledger shows, in characters.
const MAX_LINE: usize = 240;

/// The run's working memory.
#[derive(Debug, Default)]
pub(super) struct Ledger {
    /// One line per finished step, newest last.
    finished: Vec<String>,
    /// What this step tried that did not work.
    tried: Vec<String>,
}

/// What a request's memory section is built from, beside the ledger.
pub(super) struct Context<'a> {
    /// The run's history lines.
    pub(super) history: &'a [String],
    /// What this decision is for.
    pub(super) now: &'a str,
    /// The next top-level step, when there is one.
    pub(super) next: Option<&'a str>,
    /// Names of the variables set so far.
    pub(super) variables: Vec<String>,
    /// Actions and Jev calls left in the run's budgets.
    pub(super) budget_left: (u32, u32),
}

impl Ledger {
    /// Starts a step: what the last one tried no longer applies.
    pub(super) fn begin(&mut self) {
        self.tried.clear();
    }

    /// Records a finished step, already masked.
    pub(super) fn finish(&mut self, line: &str) {
        self.finished.push(clip(line));
        if self.finished.len() > MAX_FINISHED {
            self.finished.remove(0);
        }
    }

    /// Records something this step tried that did not work.
    pub(super) fn tried(&mut self, note: impl Into<String>) {
        let note = clip(&note.into());
        if self.tried.contains(&note) {
            return;
        }
        self.tried.push(note);
        if self.tried.len() > MAX_TRIED {
            self.tried.remove(0);
        }
    }

    /// The memory section of a request.
    pub(super) fn view(&self, context: &Context<'_>) -> Value {
        let history = context.history;
        let recent = history[history.len().saturating_sub(MAX_RECENT)..]
            .iter()
            .map(|line| clip(line))
            .collect::<Vec<_>>();
        let mut memory = json!({
            "steps_done": self.finished,
            "now": clip(context.now),
            "recent_actions": recent,
            "budget_left": {
                "actions": context.budget_left.0,
                "jev_calls": context.budget_left.1,
            },
        });
        if !self.tried.is_empty() {
            memory["tried_and_failed"] = json!(self.tried);
        }
        if let Some(next) = context.next {
            memory["next_step"] = json!(next);
        }
        if !context.variables.is_empty() {
            memory["variables_read"] = json!(context.variables);
        }
        memory
    }
}

fn clip(line: &str) -> String {
    if line.chars().count() <= MAX_LINE {
        return line.to_owned();
    }
    let mut clipped = line.chars().take(MAX_LINE).collect::<String>();
    clipped.push('…');
    clipped
}
