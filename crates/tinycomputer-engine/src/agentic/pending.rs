//! Goal actions held for the caller's confirmation: queueing one under a
//! random handle, and taking it back, within its time budget, when the caller
//! answers.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use tinycomputer_bus::{
    DesktopError, DesktopResponse, GoalContinuation, JevDecision, JevMetrics, JevStopReason,
    JevTurn, RunGoalRequest,
};

use super::journal::Journal;
use super::reply::{internal_error, run_response, run_response_with_id};
use super::runtime::JevRuntime;
use super::screen::{Candidate, Screen};

#[derive(Debug)]
pub(super) struct PendingRun {
    created: Instant,
    started: Instant,
    request: RunGoalRequest,
    decision: JevDecision,
    screen: Screen,
    target: Candidate,
    turns: Vec<JevTurn>,
    history: Vec<String>,
    unchanged: u32,
    metrics: JevMetrics,
    journal: Journal,
}

pub(super) fn queue_confirmation(runtime: &JevRuntime, run: PendingRun) -> DesktopResponse {
    let mut token = [0_u8; 16];
    if getrandom::fill(&mut token).is_err() {
        return internal_error("cannot create a confirmation handle");
    }
    let id = token
        .iter()
        .fold(String::with_capacity(32), |mut id, byte| {
            let _ = write!(id, "{byte:02x}");
            id
        });
    let Ok(mut pending) = runtime.pending.lock() else {
        return internal_error("confirmation state is unavailable");
    };
    pending.retain(|_, previous| previous.created.elapsed() < Duration::from_secs(600));
    if pending.len() >= 32 {
        return DesktopResponse::err(
            "run-goal",
            DesktopError::new(
                "CONFIRMATION_LIMIT",
                "too many desktop actions await confirmation",
            ),
        );
    }
    let response = run_response_with_id(
        JevStopReason::ConfirmationRequired,
        run.turns.clone(),
        Some(run.decision.clone()),
        run.metrics.clone(),
        Some(id.clone()),
    );
    pending.insert(id, run);
    response
}

pub(super) fn approved_pending(
    runtime: &JevRuntime,
    continuation: &GoalContinuation,
) -> Result<PendingRun, Box<DesktopResponse>> {
    let pending = take_pending(runtime, &continuation.id)?;
    if !continuation.approve {
        return Err(Box::new(run_response(
            JevStopReason::Cancelled,
            pending.turns,
            Some(pending.decision),
            pending.metrics,
        )));
    }
    Ok(pending)
}

pub(super) fn remaining_goal_millis(pending: &PendingRun) -> u64 {
    remaining_goal_time(pending).map_or(0, |remaining| {
        u64::try_from(remaining.as_millis()).unwrap_or(u64::MAX)
    })
}

pub(super) fn pending_stop(pending: &PendingRun, stop: JevStopReason) -> DesktopResponse {
    run_response(
        stop,
        pending.turns.clone(),
        Some(pending.decision.clone()),
        pending.metrics.clone(),
    )
}

pub(super) fn remaining_goal_time(pending: &PendingRun) -> Option<Duration> {
    Duration::from_millis(pending.request.max_elapsed_ms.clamp(1, 300_000))
        .checked_sub(pending.started.elapsed())
        .filter(|remaining| !remaining.is_zero())
}

pub(super) fn take_pending(runtime: &JevRuntime, id: &str) -> Result<PendingRun, Box<DesktopResponse>> {
    let pending = runtime
        .pending
        .lock()
        .map_err(|_| Box::new(internal_error("confirmation state is unavailable")))?
        .remove(id)
        .ok_or_else(|| {
            Box::new(DesktopResponse::err(
                "run-goal",
                DesktopError::new(
                    "CONFIRMATION_EXPIRED",
                    "confirmation handle is absent or already consumed",
                ),
            ))
        })?;
    if pending.created.elapsed() >= Duration::from_secs(600) {
        return Err(Box::new(DesktopResponse::err(
            "run-goal",
            DesktopError::new("CONFIRMATION_EXPIRED", "confirmation handle expired"),
        )));
    }
    Ok(pending)
}
