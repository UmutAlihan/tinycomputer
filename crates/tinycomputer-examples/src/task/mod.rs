//! Following a task over the bus the way an outside agent does: wait on it,
//! answer what it asks for, and collect what it did when it stops.
//!
//! `task_live` and `task_fixture` are thin on purpose — they build a
//! [`StartTaskRequest`](tinycomputer_bus::agent::StartTaskRequest) and a
//! module configuration, and everything after that is here, through
//! [`Host`] and nothing else.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use tinycomputer_bus::agent::{ContinueTaskRequest, TaskStatus, TaskView};

use crate::host::{Host, LabError};

/// The longest one `AwaitTask` call blocks before the loop looks again.
pub const AWAIT_SLICE: Duration = Duration::from_secs(30);

/// Follows the task until it stops: prints each new state, answers a
/// `needs_input` from `answers` when every field it asks for is there, and
/// cancels the task once `limit` has passed.
///
/// # Errors
///
/// Fails when a call to the module fails.
pub async fn follow(
    host: &Host,
    mut view: TaskView,
    answers: &BTreeMap<String, String>,
    limit: Duration,
) -> Result<TaskView, LabError> {
    let started = Instant::now();
    let id = view.id.clone();
    let mut last = String::new();
    loop {
        let line = format!("[{}] {}", state(&view.status), view.summary);
        if line != last {
            println!("{line}");
            last = line;
        }
        view = match &view.status {
            TaskStatus::Running => {
                let Some(wait) = next_wait(started.elapsed(), limit) else {
                    println!("time limit reached; cancelling");
                    return host.cancel_task(&id).await;
                };
                let timeout_ms = u64::try_from(wait.as_millis()).unwrap_or(u64::MAX);
                host.await_task(&id, timeout_ms).await?
            }
            TaskStatus::NeedsInput { fields } => {
                let Some(inputs) = fields
                    .iter()
                    .map(|field| {
                        answers
                            .get(&field.name)
                            .map(|value| (field.name.clone(), value.clone()))
                    })
                    .collect::<Option<BTreeMap<_, _>>>()
                else {
                    return Ok(view);
                };
                println!(
                    "  answering {}",
                    inputs.keys().cloned().collect::<Vec<_>>().join(", ")
                );
                host.continue_task(&ContinueTaskRequest {
                    id: id.clone(),
                    inputs,
                    ..ContinueTaskRequest::default()
                })
                .await?
            }
            _ => return Ok(view),
        };
    }
}

/// How long the next `AwaitTask` may block: what is left of the time limit,
/// at most one slice, so a wait never carries the run past the limit. `None`
/// once the limit is spent.
#[must_use]
pub fn next_wait(elapsed: Duration, limit: Duration) -> Option<Duration> {
    limit
        .checked_sub(elapsed)
        .filter(|left| !left.is_zero())
        .map(|left| left.min(AWAIT_SLICE))
}

/// Whether a stopped task passed: it finished, or it stopped at payment.
#[must_use]
pub fn passed(status: &TaskStatus) -> bool {
    match status {
        TaskStatus::Done { .. } => true,
        TaskStatus::Checkpoint { reason, .. } => reason.contains("payment"),
        _ => false,
    }
}

/// Collects what a stopped task did into `out`, all over the bus: the
/// report (`TaskReport`), a screenshot of each browser session still open
/// (`BrowserScreenshot` and `BrowserReadOutput`) — then closes it — and, for
/// a finished task, its records and any shaped result.
///
/// # Errors
///
/// Fails when a call to the module fails or a file cannot be written.
pub async fn conclude(host: &Host, view: &TaskView, out: &Path) -> Result<(), LabError> {
    std::fs::create_dir_all(out)?;
    let report = host.task_report(&view.id).await?;
    for step in &report.steps {
        println!(
            "  {} {} [{:?}] {}",
            step.path, step.kind, step.outcome, step.note
        );
    }
    for rescue in &report.rescues {
        println!(
            "  rescue at step {}: {:?} — {}",
            rescue.step, rescue.outcome, rescue.reason
        );
    }
    std::fs::write(
        out.join("report.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    for (index, session) in host.browser_sessions().await?.iter().enumerate() {
        let name = if index == 0 {
            "final.png".to_owned()
        } else {
            format!("final-{index}.png")
        };
        match host.browser_screenshot(&session.id).await {
            Ok(image) => {
                std::fs::write(out.join(&name), image)?;
                println!(
                    "screenshot: {} ({})",
                    out.join(&name).display(),
                    session.url
                );
            }
            Err(error) => println!("screenshot of {} failed: {error}", session.id),
        }
        host.close_browser_session(&session.id).await?;
    }
    if let TaskStatus::Done {
        records, result, ..
    } = &view.status
    {
        std::fs::write(
            out.join("records.json"),
            serde_json::to_string_pretty(records)?,
        )?;
        if let Some(result) = result {
            std::fs::write(
                out.join("result.json"),
                serde_json::to_string_pretty(result)?,
            )?;
        }
    }
    println!("final: [{}] {}", state(&view.status), view.summary);
    Ok(())
}

/// The status's wire name, such as `needs_input`.
#[must_use]
pub fn state(status: &TaskStatus) -> String {
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

#[cfg(test)]
mod test;
