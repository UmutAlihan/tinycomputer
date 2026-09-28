

//! One function per step kind; `run` dispatches between them.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use tinycomputer_bus::{
    ChooseStep, FlowAction, FlowLoop, FlowStopReason, IfStep, JevOperation, PickStep, ReadStep,
    RepeatStep, StepOutcome,
};
use tinycomputer_core::surface::{Group, result_families};
use tinycomputer_core::{Criterion, Record, rank};

use crate::workspace::BROWSER;

use super::{
    AgentBackend, Ended, FlowRun, Halt, StepLog,
    act::{DONE, SCREEN_VIEW},
    ask::{self, Questions, chosen, condition, numbered},
    backend::deliver_text,
    escalate::Belief,
    ground::Grounded,
    memory::{learn, remember},
    validate::{MAX_REPEAT, substitute_safe},
    view::{Candidate, Screen, element_kind, is_destructive, label, target_payload},
};

/// Turns a `do` step may spend.
const DO_TURNS: u32 = 8;
/// Turns spent opening the thing a `choose` step picks from.
const REVEAL_TURNS: u32 = 3;
/// Times `open` checks for a readable window, waiting between checks.
const WINDOW_CHECKS: u32 = 10;
/// Times a `wait_for` checks its condition, waiting between checks.
const WAIT_CHECKS: u32 = 10;
/// Most characters of a picked item's text kept in its variable.
const MAX_PICK_SUMMARY: usize = 400;
/// Least belief a deep run needs that a control is the one a `stop_before`
/// names before it presses it irreversibly.
pub(super) const IRREVERSIBLE_FLOOR: f64 = 0.85;
/// Least probability a `read` or `stop_before` target needs.
const LOCATE_FLOOR: f64 = 0.5;
/// How many lists an `extract` offers Jev when several show.
const MAX_LISTS: usize = 6;
/// How many of a list's first items an `extract` shows Jev to tell it apart.
const LIST_PREVIEW: usize = 3;

/// Runs one step.
pub(super) async fn run<B: AgentBackend + Sync>(
    run: &mut FlowRun<'_, B>,
    log: &mut StepLog,
    action: &FlowAction,
    text: &str,
    path: &str,
) -> Result<Ended, Halt> {
    match action {
        FlowAction::Open(app) => run.open(log, app).await,
        FlowAction::Browse(url) => run.browse(log, url).await,
        FlowAction::Do(_) => run.accomplish(log, text, DO_TURNS).await,
        FlowAction::Enter(slots) => run.enter(log, &slots.0).await,
        FlowAction::Choose(choose) => run.choose(log, choose).await,
        FlowAction::Read(read) => run.read(log, read).await,
        FlowAction::Pick(pick) => run.pick(log, pick).await,
        FlowAction::Extract(read) => run.extract(log, read).await,
        FlowAction::Verify(_) => run.verify(log, text).await,
        FlowAction::WaitFor(_) => run.wait_for(log, text).await,
        FlowAction::StopBefore(_) => run.stop_before(log, text).await,
        FlowAction::RepeatUntil(repeat) => run.repeat(log, repeat, path).await,
        FlowAction::If(branch) => run.branch(log, branch, path).await,
    }
}
