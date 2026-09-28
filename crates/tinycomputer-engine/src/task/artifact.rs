//! The screenshot a stopped run leaves: taken before the task's surfaces are
//! released, kept in `TaskReport.artifacts`, and shown on the status that
//! has room for one.

use std::time::Duration;

use tinycomputer_bus::agent::TaskStatus;

use super::FlowRunner;
use super::store::Cell;

/// How long a capture may take before the task goes on without it: a
/// screenshot is evidence, never a reason to hold a task's final state back.
pub(super) const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

/// `status` with a screenshot of the task's surface attached, when the
/// status is one a caller acts on — a checkpoint, an approval, a person's
/// turn, or the end — and the runner can take one. Every screenshot taken
/// is also kept for `TaskReport.artifacts`.
pub(super) async fn captured(cell: &Cell, runner: &dyn FlowRunner, status: TaskStatus) -> TaskStatus {
    if matches!(
        status,
        TaskStatus::Running | TaskStatus::NeedsInput { .. } | TaskStatus::NeedsPlan { .. }
    ) {
        return status;
    }
    let Some(shot) = capture(cell, runner).await else {
        return status;
    };
    match status {
        TaskStatus::NeedsApproval {
            action,
            target,
            screenshot: None,
        } => TaskStatus::NeedsApproval {
            action,
            target,
            screenshot: Some(shot),
        },
        TaskStatus::Checkpoint {
            reason,
            location,
            screenshot: None,
            summary,
            continuable,
        } => TaskStatus::Checkpoint {
            reason,
            location,
            screenshot: Some(shot),
            summary,
            continuable,
        },
        TaskStatus::NeedsHuman {
            reason,
            screenshot: None,
        } => TaskStatus::NeedsHuman {
            reason,
            screenshot: Some(shot),
        },
        other => other,
    }
}

/// Takes a screenshot of the task's surface, keeps it for the report, and
/// returns it; `None` when the runner has none to give in time.
pub(super) async fn capture(
    cell: &Cell,
    runner: &dyn FlowRunner,
) -> Option<tinycomputer_bus::browser::OutputRef> {
    let id = cell.view.borrow().id.clone();
    let shot = tokio::time::timeout(CAPTURE_TIMEOUT, runner.capture(&id))
        .await
        .ok()
        .flatten()?;
    if let Ok(mut state) = cell.state.lock() {
        state.artifacts.push(shot.clone());
    }
    Some(shot)
}
