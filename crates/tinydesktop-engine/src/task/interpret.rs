//! What a finished flow run means for the task: carry on, or pause or stop
//! with a status the caller can act on.

use tinydesktop_bus::agent::TaskStatus;
use tinydesktop_bus::{
    DesktopResponse, Flow, FlowAction, FlowRunResult, FlowStep, FlowStopReason, StepOutcome,
    StepReport,
};
use tinydesktop_core::{Consequence, consequence};

/// What the task does after a run.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Next {
    /// The run finished; go on to the next one, or finish the task.
    Continue,
    /// Stop with this status. For an approval, `resume` holds what to run
    /// once it is granted.
    Stop {
        status: TaskStatus,
        resume: Option<Resume>,
    },
}

/// What runs after an irreversible action is approved.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Resume {
    /// The `stop_before` phrase, run again with the action allowed.
    pub(super) phrase: String,
    /// The application the flow was on when it stopped.
    pub(super) app: String,
    /// The top-level steps after the paused one.
    pub(super) rest: Vec<FlowStep>,
}

/// Interprets a run of `flow`.
pub(super) fn run_outcome(flow: &Flow, reply: &DesktopResponse) -> (Next, Option<FlowRunResult>) {
    if !reply.ok {
        let (reason, hint) = reply.error.as_ref().map_or_else(
            || ("the flow could not run".to_owned(), String::new()),
            |error| {
                (
                    error.message.clone(),
                    error.suggestion.clone().unwrap_or_default(),
                )
            },
        );
        let hint = if hint.is_empty() {
            "check that Jev is configured and the surfaces are available".to_owned()
        } else {
            hint
        };
        return (failed(None, reason, hint, false), None);
    }
    let Some(result) = reply
        .data
        .clone()
        .and_then(|data| serde_json::from_value::<FlowRunResult>(data).ok())
    else {
        return (
            failed(
                None,
                "the flow runtime returned an unreadable result".to_owned(),
                String::new(),
                false,
            ),
            None,
        );
    };
    let next = match result.stop {
        FlowStopReason::Completed => Next::Continue,
        FlowStopReason::StoppedBeforeDestructive => stopped_before(flow, &result),
        FlowStopReason::StepFailed => {
            let failure = result
                .steps
                .iter()
                .rev()
                .find(|step| step.outcome == StepOutcome::Failed);
            failed(
                failure.and_then(|step| top_index(&step.path)),
                failure.map_or_else(|| "a step failed".to_owned(), |step| step.note.clone()),
                "the screen may not offer what the step describes; reword it, split it, or take over"
                    .to_owned(),
                true,
            )
        }
        FlowStopReason::ActionBudget => failed(
            None,
            "the action budget ran out".to_owned(),
            "raise budget.max_actions".to_owned(),
            true,
        ),
        FlowStopReason::ModelBudget => failed(
            None,
            "the decision budget ran out".to_owned(),
            "raise budget.max_model_calls".to_owned(),
            true,
        ),
        FlowStopReason::Invalid => failed(
            None,
            result
                .steps
                .first()
                .map_or_else(|| "the flow is invalid".to_owned(), |step| step.note.clone()),
            "fix the flow; Describe returns the guide".to_owned(),
            false,
        ),
    };
    (next, Some(result))
}

fn stopped_before(flow: &Flow, result: &FlowRunResult) -> Next {
    let gated = result
        .steps
        .iter()
        .rev()
        .find(|step| step.outcome == StepOutcome::Gated);
    let phrase = gated.map_or_else(String::new, |step| step.text.clone());
    let target = result
        .pending
        .as_ref()
        .and_then(|target| target.name.clone())
        .unwrap_or_default();
    if consequence(&target) == Consequence::Payment || consequence(&phrase) == Consequence::Payment {
        return Next::Stop {
            status: TaskStatus::Checkpoint {
                reason: format!("reached the payment step ({target}); payment is always left to you"),
                location: target,
                screenshot: None,
                summary: summary(result),
                continuable: false,
            },
            resume: None,
        };
    }
    let index = gated.and_then(|step| top_index(&step.path));
    let rest = index.map_or_else(Vec::new, |index| flow.steps[index + 1..].to_vec());
    Next::Stop {
        status: TaskStatus::NeedsApproval {
            action: phrase.clone(),
            target,
            screenshot: None,
        },
        resume: Some(Resume {
            phrase,
            app: app_at(flow, index.unwrap_or(flow.steps.len())),
            rest,
        }),
    }
}

/// The application in front at top-level step `index`: the last `open` or
/// `browse` before it, else the flow's own.
pub(super) fn app_at(flow: &Flow, index: usize) -> String {
    flow.steps[..index.min(flow.steps.len())]
        .iter()
        .rev()
        .find_map(|step| match step.action() {
            FlowAction::Open(app) => Some(app),
            FlowAction::Browse(_) => Some(crate::workspace::BROWSER.to_owned()),
            _ => None,
        })
        .unwrap_or_else(|| flow.app.clone())
}

/// The top-level index of a step path such as `3` (index 2); `None` for a
/// nested step such as `4.2`.
pub(super) fn top_index(path: &str) -> Option<usize> {
    path.parse::<usize>().ok()?.checked_sub(1)
}

/// What the finished steps did, in one line.
pub(super) fn summary(result: &FlowRunResult) -> String {
    let done = result
        .steps
        .iter()
        .filter(|step| matches!(step.outcome, StepOutcome::Done | StepOutcome::AlreadyDone))
        .map(|step| step.text.as_str())
        .collect::<Vec<_>>();
    if done.is_empty() {
        "no steps finished".to_owned()
    } else {
        format!("done: {}", done.join("; "))
    }
}

/// Top-level steps a run finished, for progress.
pub(super) fn finished(steps: &[StepReport]) -> usize {
    steps
        .iter()
        .filter(|step| {
            top_index(&step.path).is_some()
                && matches!(step.outcome, StepOutcome::Done | StepOutcome::AlreadyDone)
        })
        .count()
}

fn failed(step: Option<usize>, reason: String, hint: String, recoverable: bool) -> Next {
    Next::Stop {
        status: TaskStatus::Failed {
            step,
            reason,
            hint,
            recoverable,
        },
        resume: None,
    }
}
