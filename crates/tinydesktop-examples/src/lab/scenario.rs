//! The scenario ladder: what each task is, and how to check it really happened.
//!
//! A scenario has a plain-language brief (what an LLM author is given), a
//! hand-written high-level flow (what a person would write, with no UI
//! knowledge), a goal string for the single-loop `RunGoal` baseline, and a
//! checker that reads the application's real state — through its accessibility
//! snapshot, the filesystem, or AppleScript where no other reader exists —
//! rather than trusting the run's own report. Snapshot checks need no
//! Automation permission beyond what the lab already uses.

use std::process::Command;

use serde_json::Value;
use tinydesktop_bus::{FlowRunResult, SnapshotRequest, names};

use super::host::{Host, LabError};

/// One lab task.
#[derive(Debug, Clone, Copy)]
pub struct Scenario {
    /// Short name used on the command line.
    pub name: &'static str,
    /// The application it drives.
    pub app: &'static str,
    /// What the task is, in plain words, for an LLM author.
    pub brief: &'static str,
    /// The hand-written high-level flow.
    pub flow: &'static str,
    /// The single goal the `RunGoal` baseline is given.
    pub goal: &'static str,
    /// Texts the `RunGoal` baseline may type, in order.
    pub texts: &'static [&'static str],
    /// How to tell whether the task really happened.
    pub check: Check,
    /// What returns the application to a known state before each run, so a
    /// run cannot pass on what an earlier one left behind.
    pub reset: &'static [Reset],
}

/// One step of putting an application back into a known state.
#[derive(Debug, Clone, Copy)]
pub enum Reset {
    /// Run this AppleScript, ignoring failure.
    AppleScript(&'static str),
    /// Press this key combination at the application.
    Press(&'static str),
    /// Remove this folder from the Desktop if it exists and is empty.
    RemoveEmptyDesktopFolder(&'static str),
}

/// How a scenario is checked against the application's real state.
#[derive(Debug, Clone, Copy)]
pub enum Check {
    /// The front TextEdit document contains this text.
    TextEditContains(&'static str),
    /// The Calculator window shows this value somewhere.
    CalculatorShows(&'static str),
    /// The Notes window shows this text.
    NoteNamed(&'static str),
    /// A folder with this name exists on the Desktop.
    DesktopFolder(&'static str),
    /// The flow read the same appearance mode `defaults` reports.
    AppearanceRead,
    /// The front Mail window shows this subject and this body text.
    MailDraft {
        /// The draft's subject.
        subject: &'static str,
        /// Text the draft body must contain.
        body: &'static str,
    },
    /// The front Mail window is a reply ("Re:") draft that promises a follow up.
    MailReplyDraft,
    /// The application shows a Pause control.
    ShowsPause,
}

/// What a checker found.
#[derive(Debug, Clone)]
pub struct Verdict {
    /// Whether the real state matches the task.
    pub passed: bool,
    /// What was observed.
    pub detail: String,
}

/// Every scenario, easiest first.
pub const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "textedit",
        app: "TextEdit",
        brief: include_str!("../../scenarios/textedit/brief.md"),
        flow: include_str!("../../scenarios/textedit/flow.json"),
        goal: "Start a new blank TextEdit document and type the supplied paragraph into it.",
        texts: &[
            "Desktop flows describe what to do, not how. Jev grounds every step on the live screen. This paragraph was written by a tinydesktop flow.",
        ],
        check: Check::TextEditContains("This paragraph was written by a tinydesktop flow."),
        reset: &[Reset::AppleScript(
            r#"tell application "TextEdit" to close every document saving no"#,
        )],
    },
    Scenario {
        name: "calculator",
        app: "Calculator",
        brief: include_str!("../../scenarios/calculator/brief.md"),
        flow: include_str!("../../scenarios/calculator/flow.json"),
        goal: "Calculate 128 multiplied by 37 by pressing the Calculator buttons until the display shows 4736.",
        texts: &[],
        check: Check::CalculatorShows("4736"),
        reset: &[Reset::Press("escape"), Reset::Press("escape")],
    },
    Scenario {
        name: "notes",
        app: "Notes",
        brief: include_str!("../../scenarios/notes/brief.md"),
        flow: include_str!("../../scenarios/notes/flow.json"),
        goal: "Create a new note titled tinydesktop lab note {run} with the supplied text.",
        texts: &[
            "tinydesktop lab note {run}\nWritten by a Jev intent flow.\nSecond line: each step was grounded on the live screen.",
        ],
        check: Check::NoteNamed("tinydesktop lab note {run}"),
        reset: &[],
    },
    Scenario {
        name: "finder",
        app: "Finder",
        brief: include_str!("../../scenarios/finder/brief.md"),
        flow: include_str!("../../scenarios/finder/flow.json"),
        goal: "Make sure a folder named tinydesktop-lab exists on the Desktop, creating it if needed.",
        texts: &["tinydesktop-lab"],
        check: Check::DesktopFolder("tinydesktop-lab"),
        reset: &[Reset::RemoveEmptyDesktopFolder("tinydesktop-lab")],
    },
    Scenario {
        name: "settings-appearance",
        app: "System Settings",
        brief: include_str!("../../scenarios/settings-appearance/brief.md"),
        flow: include_str!("../../scenarios/settings-appearance/flow.json"),
        goal: "Open the Appearance settings and leave the current appearance mode visible.",
        texts: &[],
        check: Check::AppearanceRead,
        reset: &[],
    },
    Scenario {
        name: "mail-compose",
        app: "Mail",
        brief: include_str!("../../scenarios/mail-compose/brief.md"),
        flow: include_str!("../../scenarios/mail-compose/flow.json"),
        goal: "Write a new email to sam@example.com with the supplied subject and body, and stop before sending it.",
        texts: &[
            "sam@example.com",
            "Moving Thursday's sync to Friday",
            "Hi Sam,\n\nSomething came up on Thursday afternoon. Could we move our weekly sync to Friday at 3pm instead? Same agenda: the launch checklist and the open hiring loops.\n\nIf Friday doesn't work, Monday morning is also free on my side.\n\nThanks,\nAlex",
        ],
        check: Check::MailDraft {
            subject: "Friday",
            body: "3pm",
        },
        reset: &[],
    },
    Scenario {
        name: "mail-reply",
        app: "Mail",
        brief: include_str!("../../scenarios/mail-reply/brief.md"),
        flow: include_str!("../../scenarios/mail-reply/flow.json"),
        goal: "Open the newest Inbox message, start a reply, type the supplied text, and stop before sending.",
        texts: &[
            "Thanks for your note. I have read it and will follow up properly by tomorrow.\n\nBest,\nAlex",
        ],
        check: Check::MailReplyDraft,
        reset: &[],
    },
    Scenario {
        name: "spotify",
        app: "Spotify",
        brief: include_str!("../../scenarios/spotify/brief.md"),
        flow: include_str!("../../scenarios/spotify/flow.json"),
        goal: "Open Liked Songs and make sure a song is playing; choose DONE if one already is.",
        texts: &[],
        check: Check::ShowsPause,
        reset: &[],
    },
];

/// The scenario named `name`.
#[must_use]
pub fn find(name: &str) -> Option<&'static Scenario> {
    SCENARIOS.iter().find(|scenario| scenario.name == name)
}

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
            Check::AppearanceRead => {
                let dark = osascript(
                    r#"tell application "System Events" to tell appearance preferences to get dark mode"#,
                );
                let expected = if dark.trim() == "true" {
                    "dark"
                } else {
                    "light"
                };
                let read = flow
                    .and_then(|result| result.vars.get("mode"))
                    .map(|mode| mode.to_ascii_lowercase())
                    .unwrap_or_default();
                Ok(Verdict {
                    passed: read.contains(expected) || (read.contains("auto") && !read.is_empty()),
                    detail: format!("read {read:?}; the system reports {expected}"),
                })
            }
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

/// Runs AppleScript and returns its output, or the error text on failure.
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
