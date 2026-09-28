//! Continuing a goal after the caller answers a confirmation: re-observing,
//! re-finding the held target, performing the action once, and resuming the
//! goal loop with what remains of its budgets.

use std::time::{Duration, Instant};

use tinycomputer_bus::{
    DesktopResponse, GoalContinuation, JevDecision, JevMetrics, JevOperation, JevRunResult,
    JevStopReason, JevTurn,
};

use super::backend::{AgentBackend, execute_operation, observe_async};
use super::goal::{prepared_text, record_turn, same_target, within_scope};
use super::pending::{
    PendingRun, approved_pending, pending_stop, remaining_goal_millis, remaining_goal_time,
};
use super::reply::{
    action_failed_response, failed_turn, run_response, run_response_observed,
};
use super::runtime::JevRuntime;
use super::screen::{Candidate, Screen, fingerprint};
use super::task::run_goal_fresh;
use super::verify::{satisfied, verify};
use super::policy;

pub(super) async fn continue_goal<B: AgentBackend>(
    backend: B,
    runtime: JevRuntime,
    continuation: GoalContinuation,
) -> DesktopResponse {
    let pending = match approved_pending(&runtime, &continuation) {
        Ok(pending) => pending,
        Err(reply) => return *reply,
    };
    // A continuation writes to the journal of the run it continues.
    let runtime = runtime
        .within(pending.journal.clone())
        .begin_run("goal-continuation", &pending.request.goal);
    let (fresh, delivered_unverified) = match execute_pending_action(&backend, &pending).await {
        Ok(executed) => executed,
        Err(reply) => return *reply,
    };
    let mut turns = pending.turns.clone();
    let mut history = pending.history.clone();
    let confirmed = JevDecision {
        executed: true,
        ..pending.decision.clone()
    };
    let Some(remaining) = remaining_goal_time(&pending) else {
        record_turn(&mut turns, &mut history, &confirmed, false);
        return run_response(
            JevStopReason::ActionUncertain,
            turns,
            Some(confirmed),
            pending.metrics,
        );
    };
    let after = tokio::time::timeout(
        remaining,
        observe_async(
            backend.clone(),
            pending.request.app.clone(),
            pending.request.window_id.clone(),
            pending.request.root.clone(),
        ),
    )
    .await;
    let changed = if let Ok(Ok(after)) = after {
        if !within_scope(&pending.request, &after) {
            return confirmed_scope_changed(&pending, &confirmed, &mut turns, &mut history);
        }
        if let Some(reply) = confirmed_unverified_result(
            &backend,
            &pending,
            &after,
            delivered_unverified,
            &confirmed,
            &mut turns,
            &mut history,
        )
        .await
        {
            return reply;
        }
        fingerprint(&after) != fingerprint(&fresh)
    } else {
        record_turn(&mut turns, &mut history, &confirmed, false);
        return run_response(
            JevStopReason::ActionUncertain,
            turns,
            Some(confirmed),
            pending.metrics,
        );
    };
    record_turn(&mut turns, &mut history, &confirmed, changed);
    let unchanged = if changed {
        0
    } else {
        pending.unchanged.saturating_add(1)
    };
    if unchanged >= 3 {
        return run_response(JevStopReason::Stalled, turns, None, pending.metrics);
    }
    let remaining_ms = remaining_goal_millis(&pending);
    let mut request = pending.request;
    request.max_elapsed_ms = remaining_ms;
    if request.max_elapsed_ms == 0 {
        return run_response(JevStopReason::TimeBudget, turns, None, pending.metrics);
    }
    request.continuation = None;
    request.max_steps = request.max_steps.saturating_sub(1);
    if pending.decision.operation == JevOperation::TypeText
        && request.text_slots.is_empty()
        && !request.text.is_empty()
    {
        request.text.remove(0);
    }
    if request.max_steps == 0 {
        return run_response(JevStopReason::ActionBudget, turns, None, pending.metrics);
    }
    if request.max_model_calls == 0 {
        return run_response(JevStopReason::ModelBudget, turns, None, pending.metrics);
    }
    merge_continuation(
        run_goal_fresh(backend, runtime, request, history, unchanged).await,
        turns,
        &pending.metrics,
    )
}

pub(super) fn confirmed_scope_changed(
    pending: &PendingRun,
    confirmed: &JevDecision,
    turns: &mut Vec<JevTurn>,
    history: &mut Vec<String>,
) -> DesktopResponse {
    record_turn(turns, history, confirmed, false);
    run_response(
        JevStopReason::ScopeChanged,
        turns.clone(),
        Some(confirmed.clone()),
        pending.metrics.clone(),
    )
}

pub(super) async fn confirmed_unverified_result<B: AgentBackend>(
    backend: &B,
    pending: &PendingRun,
    after: &Screen,
    delivered_unverified: bool,
    confirmed: &JevDecision,
    turns: &mut Vec<JevTurn>,
    history: &mut Vec<String>,
) -> Option<DesktopResponse> {
    if !delivered_unverified || confirmed.destructive < policy::DESTRUCTIVE {
        return None;
    }
    let mut evidence = verify(after, &pending.request.success);
    if !pending.request.success.is_empty() {
        let settle_until = Instant::now() + Duration::from_secs(2);
        while !satisfied(&evidence) && Instant::now() < settle_until {
            let Some(remaining) = remaining_goal_time(pending) else {
                break;
            };
            tokio::time::sleep(Duration::from_millis(200).min(remaining)).await;
            let Some(remaining) = remaining_goal_time(pending) else {
                break;
            };
            let observed = tokio::time::timeout(
                remaining,
                observe_async(
                    backend.clone(),
                    pending.request.app.clone(),
                    pending.request.window_id.clone(),
                    pending.request.root.clone(),
                ),
            )
            .await;
            let Ok(Ok(screen)) = observed else {
                continue;
            };
            if !within_scope(&pending.request, &screen) {
                record_turn(turns, history, confirmed, false);
                return Some(run_response(
                    JevStopReason::ScopeChanged,
                    turns.clone(),
                    Some(confirmed.clone()),
                    pending.metrics.clone(),
                ));
            }
            evidence = verify(&screen, &pending.request.success);
        }
    }
    let verified = satisfied(&evidence);
    record_turn(turns, history, confirmed, verified);
    Some(run_response_observed(
        if verified {
            JevStopReason::Done
        } else {
            JevStopReason::ActionUncertain
        },
        turns.clone(),
        (!verified).then_some(confirmed.clone()),
        pending.metrics.clone(),
        Some(evidence),
    ))
}

pub(super) async fn execute_pending_action<B: AgentBackend>(
    backend: &B,
    pending: &PendingRun,
) -> Result<(Screen, bool), Box<DesktopResponse>> {
    let remaining = remaining_goal_time(pending)
        .ok_or_else(|| Box::new(pending_stop(pending, JevStopReason::TimeBudget)))?;
    let fresh = match tokio::time::timeout(
        remaining,
        observe_async(
            backend.clone(),
            pending.request.app.clone(),
            pending.request.window_id.clone(),
            pending.request.root.clone(),
        ),
    )
    .await
    {
        Ok(Ok(screen)) => screen,
        Ok(Err(error)) => return Err(error),
        Err(_) => return Err(Box::new(pending_stop(pending, JevStopReason::TimeBudget))),
    };
    let target = current_target(pending, &fresh)
        .ok_or_else(|| Box::new(pending_stop(pending, JevStopReason::StaleTarget)))?;
    let text = (pending.decision.operation == JevOperation::TypeText)
        .then(|| {
            prepared_text(&pending.request, &target)
                .or_else(|| pending.request.text.first().cloned())
        })
        .flatten();
    if pending.decision.operation == JevOperation::TypeText && text.is_none() {
        return Err(Box::new(pending_stop(pending, JevStopReason::NeedsText)));
    }
    let remaining = remaining_goal_time(pending)
        .ok_or_else(|| Box::new(pending_stop(pending, JevStopReason::TimeBudget)))?;
    let reply = tokio::time::timeout(
        remaining,
        execute_operation(
            backend.clone(),
            pending.decision.operation,
            Some(target),
            text,
        ),
    )
    .await;
    let Ok(reply) = reply else {
        let mut turns = pending.turns.clone();
        turns.push(failed_turn(&turns, &pending.decision));
        return Err(Box::new(run_response(
            JevStopReason::ActionUncertain,
            turns,
            Some(pending.decision.clone()),
            pending.metrics.clone(),
        )));
    };
    if !reply.ok {
        return Err(Box::new(action_failed_response(
            pending.turns.clone(),
            pending.decision.clone(),
            pending.metrics.clone(),
            &reply,
        )));
    }
    let delivered_unverified = reply
        .data
        .as_ref()
        .and_then(|data| data.get("disposition"))
        .and_then(|disposition| disposition.get("delivery"))
        .and_then(serde_json::Value::as_str)
        == Some("delivered_unverified");
    Ok((fresh, delivered_unverified))
}

pub(super) fn current_target(pending: &PendingRun, fresh: &Screen) -> Option<Candidate> {
    let mut matching = fresh.candidates.iter().filter(|candidate| {
        same_target(
            &pending.screen,
            fresh,
            &pending.target,
            candidate,
            pending.decision.operation,
        )
    });
    let target = matching.next()?;
    matching.next().is_none().then(|| target.clone())
}

pub(super) fn merge_continuation(
    mut result: DesktopResponse,
    mut turns: Vec<JevTurn>,
    metrics: &JevMetrics,
) -> DesktopResponse {
    if let Some(data) = result.data.take() {
        if let Ok(mut continuation_result) = serde_json::from_value::<JevRunResult>(data.clone()) {
            turns.append(&mut continuation_result.turns);
            for (index, turn) in turns.iter_mut().enumerate() {
                turn.step = u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1);
            }
            continuation_result.turns = turns;
            continuation_result.metrics.calls = continuation_result
                .metrics
                .calls
                .saturating_add(metrics.calls);
            continuation_result.metrics.attempts = continuation_result
                .metrics
                .attempts
                .saturating_add(metrics.attempts);
            continuation_result.metrics.latency_ms = continuation_result
                .metrics
                .latency_ms
                .saturating_add(metrics.latency_ms);
            continuation_result.metrics.input_tokens = continuation_result
                .metrics
                .input_tokens
                .saturating_add(metrics.input_tokens);
            continuation_result.metrics.output_tokens = continuation_result
                .metrics
                .output_tokens
                .saturating_add(metrics.output_tokens);
            result.data = serde_json::to_value(continuation_result).ok();
        } else {
            result.data = Some(data);
        }
    }
    result
}
