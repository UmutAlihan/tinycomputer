//! Putting an application back into a known state before a run, and running
//! `AppleScript`.

use std::process::Command;

use serde_json::Value;
use tinycomputer_bus::{FlowRunResult, SnapshotRequest, names};

use super::{Reset, Scenario};
use super::{Reset, Scenario};
use crate::lab::host::{Host, LabError};

impl Scenario {
    /// Launches the application and returns it to a known state.
    ///
    /// # Errors
    ///
    /// Fails only on a transport error.
    pub async fn prepare(&self, host: &Host) -> Result<(), LabError> {
        for step in self.reset {
            match step {
                Reset::AppleScript(script) => {
                    let _ = osascript(script);
                }
                Reset::Press(combo) => {
                    host.call(
                        names::methods::PRESS,
                        serde_json::json!({"combo": combo, "app": self.app}),
                    )
                    .await?;
                }
                Reset::RemoveEmptyDesktopFolder(name) => {
                    if let Ok(home) = std::env::var("HOME") {
                        let _ = std::fs::remove_dir(format!("{home}/Desktop/{name}"));
                    }
                }
            }
        }
        host.call(
            names::methods::LAUNCH,
            serde_json::json!({"app": self.app, "activate": true}),
        )
        .await?;
        tokio::time::sleep(std::time::Duration::from_millis(1_500)).await;
        Ok(())
    }
}

/// Runs `AppleScript` and returns its output, or the error text on failure.
#[must_use]
pub fn osascript(script: &str) -> String {
    match Command::new("osascript").arg("-e").arg(script).output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).into_owned()
        }
        Ok(output) => format!("error: {}", String::from_utf8_lossy(&output.stderr).trim()),
        Err(error) => format!("error: {error}"),
    }
}

async fn snapshot_text(host: &Host, app: &str) -> Result<String, LabError> {
    let reply = host
        .call(
            names::methods::SNAPSHOT,
            serde_json::to_value(SnapshotRequest {
                app: Some(app.to_owned()),
                ..SnapshotRequest::default()
            })?,
        )
        .await?;
    Ok(reply.data.map(|data| data.to_string()).unwrap_or_default())
}
