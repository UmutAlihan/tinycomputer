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

use tinycomputer_bus::agent::{ContinueTaskRequest, InputField, TaskStatus, TaskView};

use crate::host::{Host, LabError};

/// The longest one `AwaitTask` call blocks before the loop looks again.
pub const AWAIT_SLICE: Duration = Duration::from_secs(30);

/// Follows the task until it stops: prints each new state, answers a
/// `needs_input` from `answers` when every field it asks for is there, and
/// cancels the task once `limit` has passed — checked on every state it
/// reports, so an answerable pause past the limit is cancelled, not answered.
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
        if view.status.is_final() {
            return Ok(view);
        }
        let Some(wait) = next_wait(started.elapsed(), limit) else {
            println!("time limit reached; cancelling");
            return host.cancel_task(&id).await;
        };
        view = match &view.status {
            TaskStatus::Running => {
                let timeout_ms = u64::try_from(wait.as_millis()).unwrap_or(u64::MAX);
                host.await_task(&id, timeout_ms).await?
            }
            TaskStatus::NeedsInput { fields } => {
                let Some(inputs) = inputs_for(fields, answers) else {
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

/// The answers to a `needs_input` pause: one per field it asks for, or
/// `None` when any is missing — or when it asks for nothing, which no answer
/// can move past, so the pause is handed back rather than continued empty.
#[must_use]
pub fn inputs_for(
    fields: &[InputField],
    answers: &BTreeMap<String, String>,
) -> Option<BTreeMap<String, String>> {
    if fields.is_empty() {
        return None;
    }
    fields
        .iter()
        .map(|field| {
            answers
                .get(&field.name)
                .map(|value| (field.name.clone(), value.clone()))
        })
        .collect()
}

/// `url` reduced to its scheme and host, for a log line: credentials,
/// paths (password-reset and magic-link tokens ride there), queries, and
/// fragments are all dropped.
#[must_use]
pub fn loggable(url: &str) -> String {
    match url.split_once("://") {
        Some((scheme, rest)) => {
            let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
            let host = authority
                .rsplit_once('@')
                .map_or(authority, |(_, host)| host);
            format!("{scheme}://{host}")
        }
        // An opaque URL (`data:`, `about:`, a custom scheme) carries its
        // payload right after the colon, so only the scheme is kept.
        None => url
            .split_once(':')
            .map_or_else(String::new, |(scheme, _)| format!("{scheme}:")),
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
/// report (`TaskReport`); when the task managed to take one as it stopped,
/// `final.png` (`BrowserReadOutput` on the report's last artifact, which
/// `read_output` releases); otherwise an `open-<n>.png` of each browser
/// session still open; every open session is then closed; and, for a
/// finished task, its records and any shaped result. Every screenshot is
/// best effort: a task can stop without one.
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
    // The screenshot the task took when it stopped, before its session was
    // released — the only one left for a task that finished or failed. It
    // may have expired (held outputs live five minutes), so whether it was
    // actually written decides whether open sessions are captured instead.
    let mut captured = false;
    if let Some(last) = report.artifacts.last() {
        match host.read_output(last).await {
            Ok(image) => match std::fs::write(out.join("final.png"), image) {
                // Written, not merely read: only then is the fallback skipped.
                Ok(()) => {
                    captured = true;
                    println!(
                        "screenshot: {} (taken as the task stopped)",
                        out.join("final.png").display()
                    );
                }
                Err(error) => println!("final.png could not be written: {error}"),
            },
            Err(error) => println!("the task's screenshot could not be read: {error}"),
        }
    }
    // Sessions still open — a task paused at a checkpoint keeps its own — are
    // captured as they stand now, unless the task's own screenshot already
    // shows that state, then closed. Every session belongs to this run:
    // `Host::load` gives each run a private bus and its own module.
    for (index, session) in host.browser_sessions().await?.iter().enumerate() {
        if captured {
            host.close_browser_session(&session.id).await?;
            continue;
        }
        let name = format!("open-{index}.png");
        // A screenshot that cannot be taken or written is reported, never
        // allowed to skip closing the session below.
        match host.browser_screenshot(&session.id).await {
            Ok(image) => match std::fs::write(out.join(&name), image) {
                Ok(()) => println!(
                    "screenshot: {} ({})",
                    out.join(&name).display(),
                    loggable(&session.url)
                ),
                Err(error) => println!("{name} could not be written: {error}"),
            },
            Err(error) => println!("screenshot of {} failed: {error}", session.id),
        }
        host.close_browser_session(&session.id).await?;
    }
    if !captured && !out.join("open-0.png").exists() {
        println!("no screenshot: the task's surface could not take one");
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
mod task_tests;
