//! Telling a wall only a person can pass from an ordinary failure.

use tinycomputer_bus::agent::TaskStatus;

use super::FlowRunner;
use super::interpret::Resume;
use super::store::Cell;

/// A recoverable failure in front of something only a person can pass — a
/// captcha, a one-time code, a login wall — becomes `needs_human`, and the
/// failed step runs again once they have. Anything else stays a failure.
pub(super) async fn human_wall(
    cell: &Cell,
    runner: &dyn FlowRunner,
    status: TaskStatus,
) -> TaskStatus {
    let retryable = matches!(
        status,
        TaskStatus::Failed {
            recoverable: true,
            ..
        }
    ) && cell
        .state
        .lock()
        .is_ok_and(|state| matches!(state.resume, Some(Resume::Retry { .. })));
    let wall = if retryable {
        let id = cell.view.borrow().id.clone();
        tinycomputer_core::human_needed(&runner.visible_text(&id).await)
    } else {
        None
    };
    if let Some(action) = wall {
        return TaskStatus::NeedsHuman {
            reason: format!("{action}, then continue the task"),
            screenshot: None,
        };
    }
    // No person can help, so there is nothing to retry.
    if let Ok(mut state) = cell.state.lock()
        && matches!(state.resume, Some(Resume::Retry { .. }))
    {
        state.resume = None;
    }
    status
}
