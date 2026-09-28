//! Running a task in the background: planning it when asked to, running its
//! flows in order, interpreting where each stopped, and finishing it.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tinycomputer_bus::agent::TaskStatus;

use super::artifact::{capture, captured};
use super::budget::{elapsed_budget_failed, run_request, stop_task};
use super::human::human_wall;
use super::interpret::{Next, finished, run_outcome};
use super::publish::{publish, records, stopped_summary};
use super::recovery::{rescue_outcome, rescued};
use super::store::{Cell, Run};
use super::{FlowRunner, SHAPE_TIMEOUT_MS};
use crate::shape::Harvest;

/// Plans the task, then runs the plan — or asks for what it needs first.
pub(super) async fn plan_then_drive(
    cell: Arc<Cell>,
    runner: Arc<dyn FlowRunner>,
    planner: crate::planner::Planner,
    task: String,
    surfaces: Vec<tinycomputer_bus::agent::SurfaceKind>,
) {
    let (names, secrets) = cell.state.lock().map_or_else(
        |_| (Vec::new(), Vec::new()),
        |state| {
            let owned = |names: Vec<&str>| names.into_iter().map(str::to_owned).collect::<Vec<_>>();
            (
                owned(state.facts.names()),
                owned(state.facts.secret_names()),
            )
        },
    );
    let plan = match planner.plan(&task, &names, &secrets, &surfaces).await {
        Ok(plan) => plan,
        Err(reason) => {
            publish(
                &cell,
                TaskStatus::Failed {
                    step: None,
                    reason: reason.clone(),
                    hint: "reword the task, or pass a flow written with Describe's guide"
                        .to_owned(),
                    recoverable: true,
                },
                &format!("Planning failed: {reason}"),
            );
            return;
        }
    };
    let allow = {
        let Ok(mut state) = cell.state.lock() else {
            return;
        };
        state.flow = plan.flow.clone();
        state.constraints.allow_destructive
    };
    if plan.questions.is_empty() {
        drive(
            cell,
            runner,
            vec![Run {
                flow: plan.flow,
                allow_destructive: allow,
                rescue: None,
            }],
        )
        .await;
    } else {
        publish(
            &cell,
            TaskStatus::NeedsInput {
                fields: plan.questions,
            },
            "The plan needs values before it can start.",
        );
    }
}

/// Runs a task's flows in order until one stops it or all finish.
///
/// A task's [`TaskBudget`] bounds the whole task, not one run of it: an
/// approval or a human intervention splits a task into several runs, and
/// each is given only what the task has not already spent, so resuming can
/// never reset the budget back to full. `max_elapsed_ms` has no equivalent in
/// [`RunFlowRequest`] — a run cannot police its own wall-clock time from the
/// inside — so it is enforced here instead, by timing out a run that would
/// otherwise run past what remains of it.
pub(super) async fn drive(cell: Arc<Cell>, runner: Arc<dyn FlowRunner>, runs: Vec<Run>) {
    let mut runs = VecDeque::from(runs);
    while let Some(run) = runs.pop_front() {
        let Some((request, constraints, time_left)) = run_request(&cell, &run) else {
            return;
        };
        if time_left == Some(0) {
            stop_task(&cell, runner.as_ref(), elapsed_budget_failed()).await;
            return;
        }
        let id = cell.view.borrow().id.clone();
        let started = Instant::now();
        let run_call = runner.run(&id, &constraints, request);
        let reply = if let Some(ms) = time_left {
            let Ok(reply) = tokio::time::timeout(Duration::from_millis(ms), run_call).await else {
                stop_task(&cell, runner.as_ref(), elapsed_budget_failed()).await;
                return;
            };
            reply
        } else {
            run_call.await
        };
        let spent_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let (next, result) = run_outcome(&run.flow, &reply, constraints.payment);
        let reached = result
            .as_ref()
            .map_or_else(Vec::new, |result| result.steps.clone());
        let redacted = {
            let Ok(mut state) = cell.state.lock() else {
                return;
            };
            state.spent.elapsed_ms = state.spent.elapsed_ms.saturating_add(spent_ms);
            if let Some(rescue) = run.rescue.and_then(|index| state.rescues.get_mut(index)) {
                rescue.outcome = rescue_outcome(&reached, rescue.steps.len());
            }
            if let Some(result) = result {
                state.spent.actions = state.spent.actions.saturating_add(result.actions);
                state.spent.model_calls =
                    state.spent.model_calls.saturating_add(result.metrics.calls);
                state.finished += finished(&result.steps);
                state.steps.extend(result.steps);
                state.exchanges.extend(result.trace);
                state.learned.extend(result.learned);
                for (name, value) in result.vars {
                    if state.facts.get(&name).is_none() && !run.flow.vars.contains_key(&name) {
                        state.reads.insert(name, value);
                    }
                }
            }
            if let Next::Stop { resume, .. } = &next {
                state.resume.clone_from(resume);
            }
            state.facts.clone()
        };
        if let Next::Stop { status, .. } = next {
            let status = human_wall(&cell, runner.as_ref(), *status).await;
            let status = match rescued(&cell, runner.as_ref(), &run, status, reached).await {
                Ok(status) => status,
                Err(rescued) => {
                    runs.push_front(rescued);
                    continue;
                }
            };
            let status = captured(&cell, runner.as_ref(), status).await;
            let summary = redacted.redact(&stopped_summary(&status));
            let ended = matches!(
                status,
                TaskStatus::Done { .. } | TaskStatus::Failed { .. } | TaskStatus::Cancelled
            );
            publish(&cell, status, &summary);
            if ended {
                runner.release(&cell.view.borrow().id);
            }
            return;
        }
    }
    finish(&cell, runner.as_ref()).await;
}

/// Every run finished: the task is done, with what it read, shaped as its
/// `output` asks when it asks.
pub(super) async fn finish(cell: &Cell, runner: &dyn FlowRunner) {
    // The last look at the surface, before shaping and before release.
    let _ = capture(cell, runner).await;
    let (answer, records, harvest) = {
        let Ok(state) = cell.state.lock() else {
            return;
        };
        let reads = state
            .reads
            .iter()
            .map(|(name, value)| format!("{name}: {value}"))
            .collect::<Vec<_>>();
        let answer = if reads.is_empty() {
            format!("Finished all {} steps.", state.flow.steps.len())
        } else {
            format!(
                "Finished all {} steps. {}",
                state.flow.steps.len(),
                reads.join("; ")
            )
        };
        let harvest = state.output.clone().map(|output| Harvest {
            goal: state.facts.redact(&state.goal),
            output,
            reads: state
                .reads
                .iter()
                .map(|(name, value)| (name.clone(), state.facts.redact(value)))
                .collect(),
        });
        (state.facts.redact(&answer), records(&state.reads), harvest)
    };
    let result = match (harvest, cell.shaper.clone()) {
        (Some(harvest), Some(shaper)) => {
            publish(
                cell,
                TaskStatus::Running,
                "Every step finished; shaping the answer.",
            );
            let answered = tokio::time::timeout(
                Duration::from_millis(SHAPE_TIMEOUT_MS),
                shaper.shape(&harvest),
            )
            .await
            .unwrap_or_else(|_| Err("shaping the answer took too long".to_owned()));
            match answered {
                Ok(result) => Some(result),
                Err(why) => {
                    let steps = cell.state.lock().map_or(0, |state| state.flow.steps.len());
                    let reason = format!("the answer could not be shaped: {why}");
                    publish(
                        cell,
                        TaskStatus::Failed {
                            step: Some(steps),
                            reason: reason.clone(),
                            hint: "every step finished; TaskReport holds the records they read"
                                .to_owned(),
                            recoverable: false,
                        },
                        &format!("The task failed: {reason}"),
                    );
                    runner.release(&cell.view.borrow().id);
                    return;
                }
            }
        }
        _ => None,
    };
    publish(
        cell,
        TaskStatus::Done {
            answer: answer.clone(),
            records,
            result,
        },
        &answer,
    );
    runner.release(&cell.view.borrow().id);
}
