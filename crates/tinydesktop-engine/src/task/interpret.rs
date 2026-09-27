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
        status: Box<TaskStatus>,
        resume: Option<Resume>,
    },
}

/// What runs when a paused task is continued.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Resume {
    /// Perform the approved irreversible action, then the steps after it.
    Approval {
        /// The `stop_before` phrase, run again with the action allowed.
        phrase: String,
        /// The application the flow was on when it stopped.
        app: String,
        /// The top-level steps after the paused one.
        rest: Vec<FlowStep>,
    },
    /// Run the failed step again, and everything after it, once a person has
    /// got past what blocked it.
    Retry {
        /// The application the flow was on when it failed.
        app: String,
        /// The failed top-level step and the ones after it.
        steps: Vec<FlowStep>,
    },
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
            let index = failure.and_then(|step| top_index(&step.path));
            let mut next = failed(
                index,
                failure.map_or_else(|| "a step failed".to_owned(), |step| step.note.clone()),
                "the screen may not offer what the step describes; reword it, split it, or take over"
                    .to_owned(),
                true,
            );
            if let (Some(index), Next::Stop { resume, .. }) = (index, &mut next) {
                *resume = Some(Resume::Retry {
                    app: app_at(flow, index),
                    steps: flow.steps[index..].to_vec(),
                });
            }
            next
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
            result.steps.first().map_or_else(
                || "the flow is invalid".to_owned(),
                |step| step.note.clone(),
            ),
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
    if consequence(&target) == Consequence::Payment || consequence(&phrase) == Consequence::Payment
    {
        return Next::Stop {
            status: Box::new(TaskStatus::Checkpoint {
                reason: format!(
                    "reached the payment step ({target}); payment is always left to you"
                ),
                location: target,
                screenshot: None,
                summary: summary(result),
                continuable: false,
            }),
            resume: None,
        };
    }
    let path = gated.map(|step| step.path.as_str());
    let index = path.and_then(top_index);
    // A top-level `stop_before` has fully finished once it is approved, so
    // the rest resumes right after it. One nested in an `if` or
    // `repeat_until` (path `4.2` or `4.r1.2`) has not: that whole top-level
    // step is still in progress, and which branch or round it was in is not
    // recoverable from the path alone, so the containing step is resumed
    // from its own start rather than silently dropped along with everything
    // that follows it.
    let rest = match (index, path) {
        (Some(index), Some(path)) if path.contains('.') => flow.steps[index..].to_vec(),
        (Some(index), _) => flow.steps[index + 1..].to_vec(),
        (None, _) => Vec::new(),
    };
    Next::Stop {
        status: Box::new(TaskStatus::NeedsApproval {
            action: phrase.clone(),
            target,
            screenshot: None,
        }),
        resume: Some(Resume::Approval {
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
        status: Box::new(TaskStatus::Failed {
            step,
            reason,
            hint,
            recoverable,
        }),
        resume: None,
    }
}
