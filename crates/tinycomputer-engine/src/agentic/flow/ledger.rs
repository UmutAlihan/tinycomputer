//! The run's working memory, as every wide-strategy question sees it.
//!
//! The narrow strategy shows Jev the last twenty history lines, a mix of
//! step outcomes, change notes, and runtime notes, and older steps fall out
//! of it. The ledger keeps what a person carrying out the task would keep in
//! mind instead:
//!
//! - every finished step, one line each, for the whole run;
//! - everything that happened in the current step, turn by turn;
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
/// Lines of the current step the ledger shows, newest last.
const MAX_STEP_LINES: usize = 40;
/// Tried-and-failed notes kept for one step.
const MAX_TRIED: usize = 12;
/// Longest line the ledger shows, in characters.
const MAX_LINE: usize = 240;

/// The run's working memory.
#[derive(Debug, Default)]
pub(super) struct Ledger {
    /// One line per finished step, newest last.
    finished: Vec<String>,
    /// Where the current step's lines begin in the run's history.
    step_start: usize,
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
    /// Starts a step whose lines begin at `history_len`.
    pub(super) fn begin(&mut self, history_len: usize) {
        self.step_start = history_len;
        self.tried.clear();
    }

    /// Records a finished step, already masked.
    pub(super) fn finish(&mut self, line: String) {
        self.finished.push(clip(&line));
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
        let start = self.step_start.min(context.history.len());
        let this_step = &context.history[start..];
        let this_step = this_step[this_step.len().saturating_sub(MAX_STEP_LINES)..]
            .iter()
            .map(|line| clip(line))
            .collect::<Vec<_>>();
        let mut memory = json!({
            "steps_done": self.finished,
            "now": context.now,
            "this_step": this_step,
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
