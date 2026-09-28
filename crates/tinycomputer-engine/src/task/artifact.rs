//! The screenshot a stopped run leaves: taken before the task's surfaces are
//! released, and kept in `TaskReport.artifacts` only.
//!
//! It never goes on a status. A `TaskView` also travels through `AwaitTask`
//! and `ListTasks`, which are not confidential, and an output handle is a
//! bearer token for `BrowserReadOutput`: a screenshot of a filled traveller
//! or payment form must only be reachable through the confidential report.

use std::time::Duration;

use tinycomputer_bus::agent::TaskStatus;

use super::FlowRunner;
use super::store::Cell;

/// How long a capture may take before the task goes on without it: a
/// screenshot is evidence, never a reason to hold a task's final state back.
pub(super) const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

/// `status`, unchanged, after keeping a screenshot of the task's surface for
/// `TaskReport.artifacts` — when the status is one a caller acts on (a
/// checkpoint, an approval, a person's turn, or the end) and the runner can
/// take one.
pub(super) async fn captured(
    cell: &Cell,
    runner: &dyn FlowRunner,
    status: TaskStatus,
) -> TaskStatus {
    if !matches!(
        status,
        TaskStatus::Running | TaskStatus::NeedsInput { .. } | TaskStatus::NeedsPlan { .. }
    ) {
        let _kept = capture(cell, runner).await;
    }
    status
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
