//! Following a task until it stops, and cancelling it past the time limit.

use std::time::{Duration, Instant};

use tinycomputer_bus::agent::{AwaitTaskRequest, TaskStatus, TaskView};
use tinycomputer_engine::Tasks;

/// Follows the task until it stops, cancelling it past the time limit.
pub(crate) async fn follow(tasks: &Tasks, mut view: TaskView) -> Result<TaskView, String> {
    let limit = Duration::from_secs(
        60 * std::env::var("TASK_MAX_MINUTES")
            .ok()
            .and_then(|minutes| minutes.parse().ok())
            .unwrap_or(20),
    );
    let started = Instant::now();
    let id = view.id.clone();
    let mut last = String::new();
    loop {
        let line = format!("[{}] {}", state(&view.status), view.summary);
        if line != last {
            println!("{line}");
            last = line;
        }
        if !matches!(view.status, TaskStatus::Running) {
            return Ok(view);
        }
        let Some(wait) = next_wait(started.elapsed(), limit) else {
            println!("time limit reached; cancelling");
            return tasks
                .cancel(&id)
                .data
                .ok_or_else(|| "the task could not be cancelled".to_owned());
        };
        let reply = tasks
            .await_task(AwaitTaskRequest {
                id: id.clone(),
                timeout_ms: u64::try_from(wait.as_millis()).unwrap_or(u64::MAX),
            })
            .await;
        view = reply
            .data
            .ok_or_else(|| format!("the task stopped answering: {:?}", reply.error))?;
    }
}

/// The longest one `AwaitTask` call blocks before the loop looks again.
pub(crate) const AWAIT_SLICE: Duration = Duration::from_secs(30);

/// How long the next `AwaitTask` may block: what is left of the time limit,
/// at most one slice, so a wait never carries the run past the limit. `None`
/// once the limit is spent.
pub(crate) fn next_wait(elapsed: Duration, limit: Duration) -> Option<Duration> {
    limit
        .checked_sub(elapsed)
        .filter(|left| !left.is_zero())
        .map(|left| left.min(AWAIT_SLICE))
}

pub(crate) fn state(status: &TaskStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| {
            value
                .get("state")
                .and_then(|state| state.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_default()
}
