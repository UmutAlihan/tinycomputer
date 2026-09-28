//! `RunGoal`'s entry point, and the scope and turn bookkeeping the goal loop
//! and its continuations share.

use tinycomputer_bus::{
    DesktopResponse, JevDecision, JevDecisionKind, JevOperation, JevStopReason, JevTurn,
    RunGoalRequest,
};

use super::backend::AgentBackend;
use super::continuation::continue_goal;
use super::runtime::JevRuntime;
use super::screen::{Candidate, Screen};
use super::task::run_goal_fresh;
use super::verify::exact_label;
use crate::Desktop;

/// Runs a bounded, scoped `RunGoal` task on the live desktop.
pub async fn run_goal(
    desktop: Desktop,
    runtime: JevRuntime,
    request: RunGoalRequest,
) -> DesktopResponse {
    run_goal_with(desktop, runtime, request).await
}

pub(super) async fn run_goal_with<B: AgentBackend>(
    backend: B,
    runtime: JevRuntime,
    request: RunGoalRequest,
) -> DesktopResponse {
    if let Some(continuation) = request.continuation.clone() {
        return continue_goal(backend, runtime, continuation).await;
    }
    let runtime = runtime.begin_run("goal", &format!("{}: {}", request.app, request.goal));
    run_goal_fresh(backend, runtime, request, Vec::new(), 0).await
}

pub(super) fn selected_target(screen: &Screen, decision: &JevDecision) -> Option<Candidate> {
    decision
        .target
        .as_ref()
        .and_then(|target| {
            screen
                .candidates
                .iter()
                .find(|candidate| candidate.ref_id == target.ref_id)
        })
        .cloned()
}

pub(super) fn within_scope(request: &RunGoalRequest, screen: &Screen) -> bool {
    request.app.eq_ignore_ascii_case(&screen.app)
        && request
            .window_id
            .as_deref()
            .is_none_or(|window_id| screen.window_id.as_deref() == Some(window_id))
        && request
            .window
            .as_deref()
            .is_none_or(|window| screen.window.as_deref() == Some(window))
}

pub(super) fn mutates(operation: JevOperation) -> bool {
    matches!(
        operation,
        JevOperation::Click
            | JevOperation::TypeText
            | JevOperation::Check
            | JevOperation::Uncheck
            | JevOperation::Expand
            | JevOperation::Collapse
            | JevOperation::Scroll
    )
}

pub(super) fn target_allowed(request: &RunGoalRequest, candidate: &Candidate) -> bool {
    request.allowed_targets.is_empty()
        || request
            .allowed_targets
            .iter()
            .any(|name| exact_label(candidate, name))
}

pub(super) fn prepared_text(request: &RunGoalRequest, candidate: &Candidate) -> Option<String> {
    request
        .text_slots
        .iter()
        .find(|(name, _)| exact_label(candidate, name))
        .map(|(_, value)| value.clone())
}

pub(super) fn record_turn(
    turns: &mut Vec<JevTurn>,
    history: &mut Vec<String>,
    decision: &JevDecision,
    changed: bool,
) {
    let turn = JevTurn {
        step: u32::try_from(turns.len())
            .unwrap_or(u32::MAX)
            .saturating_add(1),
        operation: decision.operation,
        target: decision.target.clone(),
        confidence: decision.confidence,
        ok: decision.executed,
        changed,
    };
    history.push(format!(
        "step {}: {:?} {} and changed={changed}",
        turn.step,
        turn.operation,
        turn.target
            .as_ref()
            .and_then(|target| target.name.as_deref())
            .unwrap_or("the selected element")
    ));
    turns.push(turn);
}

pub(super) fn same_target(
    before: &Screen,
    after: &Screen,
    old: &Candidate,
    current: &Candidate,
    operation: JevOperation,
) -> bool {
    let action = match operation {
        JevOperation::Click => "Click",
        JevOperation::TypeText => "SetValue",
        JevOperation::Check | JevOperation::Uncheck => "Toggle",
        JevOperation::Expand => "Expand",
        JevOperation::Collapse => "Collapse",
        JevOperation::Scroll => "Scroll",
        JevOperation::Drill => "Drill",
        _ => return false,
    };
    before.app == after.app
        && before.window == after.window
        && before.window_id == after.window_id
        && before.surface == after.surface
        && old.role == current.role
        && old.name == current.name
        && old.description == current.description
        && old.native_id == current.native_id
        && old.path == current.path
        && old.bounds == current.bounds
        && old.states == current.states
        && (old.label().is_some() || old.bounds.is_some())
        && (operation == JevOperation::Drill
            || current.available_actions.iter().any(|available| {
                available == action
                    || (operation == JevOperation::TypeText && available == "TypeText")
            }))
}

pub(super) fn stop_reason(decision: JevDecisionKind) -> Option<JevStopReason> {
    match decision {
        JevDecisionKind::Done => Some(JevStopReason::Done),
        JevDecisionKind::Blocked => Some(JevStopReason::Blocked),
        JevDecisionKind::ConfirmationRequired => Some(JevStopReason::ConfirmationRequired),
        JevDecisionKind::Abstain => Some(JevStopReason::LowConfidence),
        JevDecisionKind::NeedsText => Some(JevStopReason::NeedsText),
        JevDecisionKind::Act => None,
    }
}
