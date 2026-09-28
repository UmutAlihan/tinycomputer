//! The scenario ladder: what each task is, and how to check it really happened.
//!
//! A scenario has a plain-language brief (what an LLM author is given), a
//! hand-written high-level flow (what a person would write, with no UI
//! knowledge), a goal string for the single-loop `RunGoal` baseline, and a
//! checker that reads the application's real state — through its accessibility
//! snapshot, the filesystem, or `AppleScript` where no other reader exists —
//! rather than trusting the run's own report. Snapshot checks need no
//! Automation permission beyond what the lab already uses.

mod check;
mod ladder;
mod prepare;

pub use ladder::SCENARIOS;
pub use prepare::osascript;

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
    /// Run this `AppleScript`, ignoring failure.
    AppleScript(&'static str),
    /// Press this key combination at the application.
    Press(&'static str),
    /// Remove this folder from the Desktop if it exists and is empty.
    RemoveEmptyDesktopFolder(&'static str),
}

/// How a scenario is checked against the application's real state.
#[derive(Debug, Clone, Copy)]
pub enum Check {
    /// The front `TextEdit` document contains this text.
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

/// The scenario named `name`.
#[must_use]
pub fn find(name: &str) -> Option<&'static Scenario> {
    SCENARIOS.iter().find(|scenario| scenario.name == name)
}
