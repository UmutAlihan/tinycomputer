//! What a caller sees of a task: its published view, the summary of where it
//! stopped, the calls it may make next, and the records it read.

use std::collections::BTreeMap;

use tinycomputer_bus::agent::{InputField, StepView, TaskStatus};

use super::interpret;
use super::names::input_kind;
use super::store::Cell;

pub(super) fn stopped_summary(status: &TaskStatus) -> String {
    match status {
        TaskStatus::NeedsHuman { reason, .. } => format!("A person is needed: {reason}."),
        TaskStatus::NeedsApproval { action, target, .. } => {
            format!(
                "Stopped before an irreversible action ({action}: {target}); approve or decline it."
            )
        }
        TaskStatus::Checkpoint { reason, .. } => format!("Stopped: {reason}."),
        TaskStatus::Failed { reason, .. } => format!("The task failed: {reason}"),
        other => format!("The task is {}.", state_name(other)),
    }
}

/// Updates a task's view: status, summary, progress, step, and next calls.
pub(super) fn publish(cell: &Cell, status: TaskStatus, summary: &str) {
    let (progress, step) = cell.state.lock().map_or((0.0, None), |state| {
        let total = state.flow.steps.len().max(1);
        let fraction = |count: usize| f32::from(u16::try_from(count).unwrap_or(u16::MAX));
        let progress = fraction(state.finished.min(total)) / fraction(total);
        let step = state.steps.last().map(|report| StepView {
            index: interpret::top_index(report.path.split('.').next().unwrap_or("1")).unwrap_or(0),
            total: state.flow.steps.len(),
            kind: report.kind.clone(),
            intent: state.facts.redact(&report.text),
            surface: interpret::app_at(
                &state.flow,
                interpret::top_index(report.path.split('.').next().unwrap_or("1")).unwrap_or(0),
            ),
        });
        (progress, step)
    });
    cell.view.send_modify(|view| {
        view.next = next_calls(&status);
        view.status = status;
        summary.clone_into(&mut view.summary);
        view.progress = progress;
        view.step = step;
    });
}

pub(super) fn needs_input(missing: &[String]) -> TaskStatus {
    TaskStatus::NeedsInput {
        fields: missing
            .iter()
            .map(|name| InputField {
                name: name.clone(),
                why: format!("the flow uses ${{{name}}} and no fact supplies it"),
                kind: input_kind(name),
                options: Vec::new(),
            })
            .collect(),
    }
}

pub(super) fn records(reads: &BTreeMap<String, String>) -> BTreeMap<String, Vec<BTreeMap<String, String>>> {
    reads
        .iter()
        .map(|(name, value)| {
            // An `extract` stores JSON rows of text; anything else is one value.
            let rows = serde_json::from_str::<Vec<Vec<String>>>(value).map_or_else(
                |_| vec![BTreeMap::from([("value".to_owned(), value.clone())])],
                |rows| {
                    rows.into_iter()
                        .map(|fields| {
                            fields
                                .into_iter()
                                .enumerate()
                                .map(|(index, field)| (format!("field {}", index + 1), field))
                                .collect()
                        })
                        .collect()
                },
            );
            (name.clone(), rows)
        })
        .collect()
}

pub(super) fn next_calls(status: &TaskStatus) -> Vec<String> {
    let calls: &[&str] = match status {
        TaskStatus::Running => &["AwaitTask", "CancelTask"],
        TaskStatus::NeedsInput { .. } | TaskStatus::NeedsApproval { .. } => {
            &["ContinueTask", "CancelTask"]
        }
        TaskStatus::NeedsHuman { .. } => &["ContinueTask", "CancelTask", "TaskReport"],
        TaskStatus::Checkpoint {
            continuable: true, ..
        } => &["ContinueTask", "TaskReport"],
        TaskStatus::NeedsPlan { .. } => &["StartTask"],
        TaskStatus::Failed { .. } => &["TaskReport", "StartTask"],
        // A final checkpoint's workspace is only ever released by
        // `CancelTask`, so it must stay offered even though the task is done.
        TaskStatus::Checkpoint { .. } => &["CancelTask", "TaskReport"],
        TaskStatus::Done { .. } | TaskStatus::Cancelled => &["TaskReport"],
    };
    calls.iter().map(|call| (*call).to_owned()).collect()
}

pub(super) fn state_name(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Running => "running",
        TaskStatus::NeedsInput { .. } => "needs_input",
        TaskStatus::NeedsApproval { .. } => "needs_approval",
        TaskStatus::Checkpoint { .. } => "checkpoint",
        TaskStatus::NeedsHuman { .. } => "needs_human",
        TaskStatus::NeedsPlan { .. } => "needs_plan",
        TaskStatus::Done { .. } => "done",
        TaskStatus::Failed { .. } => "failed",
        TaskStatus::Cancelled => "cancelled",
    }
}
