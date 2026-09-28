//! Checking a run against the application's real state.

use serde_json::Value;
use tinycomputer_bus::{FlowRunResult, SnapshotRequest, names};

use super::{Check, Scenario, Verdict, osascript};
use crate::lab::host::{Host, LabError};

impl Scenario {
    /// The hand-written flow, parsed.
    ///
    /// # Errors
    ///
    /// Fails when the checked-in flow is not JSON.
    pub fn flow_json(&self) -> Result<Value, LabError> {
        Ok(serde_json::from_str(self.flow)?)
    }

    /// Checks the application's real state after a run.
    ///
    /// # Errors
    ///
    /// Fails when the state cannot be read at all.
    pub async fn verdict(
        &self,
        host: &Host,
        flow: Option<&FlowRunResult>,
        run: &str,
    ) -> Result<Verdict, LabError> {
        match self.check {
            Check::TextEditContains(text) => Ok(contains(
                &osascript(
                    r#"tell application "TextEdit" to if (count of documents) > 0 then get text of front document"#,
                ),
                text,
            )),
            Check::CalculatorShows(value) => {
                let tree = snapshot_text(host, self.app).await?;
                let shown = tree.contains(value) || tree.contains(&thousands(value));
                Ok(Verdict {
                    passed: shown,
                    detail: format!(
                        "the Calculator tree {} {value}",
                        if shown { "shows" } else { "does not show" }
                    ),
                })
            }
            Check::NoteNamed(name) => Ok(contains(
                &snapshot_text(host, self.app).await?,
                &name.replace("{run}", run),
            )),
            Check::DesktopFolder(name) => {
                let path = std::env::var("HOME").map(|home| format!("{home}/Desktop/{name}"))?;
                let exists = std::path::Path::new(&path).is_dir();
                Ok(Verdict {
                    passed: exists,
                    detail: format!("{path} {}", if exists { "exists" } else { "is missing" }),
                })
            }
            Check::AppearanceRead => Ok(appearance_read_verdict(flow)),
            Check::MailDraft { subject, body } => {
                let window = snapshot_text(host, self.app).await?;
                Ok(Verdict {
                    passed: window.contains(subject) && window.contains(body),
                    detail: format!(
                        "the front Mail window {} the subject and {} the body",
                        if window.contains(subject) {
                            "shows"
                        } else {
                            "lacks"
                        },
                        if window.contains(body) {
                            "shows"
                        } else {
                            "lacks"
                        },
                    ),
                })
            }
            Check::MailReplyDraft => {
                let window = snapshot_text(host, self.app).await?;
                Ok(Verdict {
                    passed: window.contains("Re:") && window.contains("follow up"),
                    detail: format!(
                        "the front Mail window {} a reply draft",
                        if window.contains("Re:") {
                            "is"
                        } else {
                            "is not"
                        }
                    ),
                })
            }
            Check::ShowsPause => {
                let tree = snapshot_text(host, self.app).await?;
                let playing = tree.contains("\"Pause\"");
                Ok(Verdict {
                    passed: playing,
                    detail: format!(
                        "a Pause control is {}",
                        if playing { "showing" } else { "absent" }
                    ),
                })
            }
        }
    }
}

/// Checks the flow's read appearance mode against what `defaults` reports.
///
/// Accepts a read only when it names exactly one of `light`, `dark`, or
/// `auto`: a read that mentions every mode, or one `osascript` could not
/// answer, must not report a false pass.
fn appearance_read_verdict(flow: Option<&FlowRunResult>) -> Verdict {
    let dark = osascript(
        r#"tell application "System Events" to tell appearance preferences to get dark mode"#,
    );
    let expected = match dark.trim() {
        "true" => "dark",
        "false" => "light",
        other => {
            return Verdict {
                passed: false,
                detail: format!("could not read the system appearance: {other}"),
            };
        }
    };
    let read = flow
        .and_then(|result| result.vars.get("mode"))
        .map(|mode| mode.to_ascii_lowercase())
        .unwrap_or_default();
    let named = ["light", "dark", "auto"]
        .into_iter()
        .filter(|mode| read.contains(mode))
        .collect::<Vec<_>>();
    Verdict {
        passed: named.len() == 1 && (named[0] == expected || named[0] == "auto"),
        detail: format!("read {read:?}; the system reports {expected}"),
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

fn contains(observed: &str, expected: &str) -> Verdict {
    Verdict {
        passed: observed.contains(expected),
        detail: format!("found {}", clip(observed)),
    }
}

fn thousands(value: &str) -> String {
    let digits = value.chars().rev().collect::<Vec<_>>();
    let mut out = String::new();
    for (index, digit) in digits.iter().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push(',');
        }
        out.push(*digit);
    }
    out.chars().rev().collect()
}

fn clip(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 240 {
        format!("{}…", flat.chars().take(240).collect::<String>())
    } else {
        flat
    }
}
