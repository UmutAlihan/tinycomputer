//! Judging the rescuer's answer: reading its guidance, refusing what would
//! drop a guard or leave one out, and the flow that runs once it is accepted.

use serde_json::Value;
use tinycomputer_bus::{Flow, FlowAction, FlowStep};

use super::{Briefing, Guidance, MAX_RESCUE_STEPS};
use crate::planner::json_object;

/// The guidance in `reply`, or what is wrong with it.
pub(super) fn judge(reply: &str, briefing: &Briefing) -> Result<Guidance, String> {
    let value = json_object(reply).map_err(|error| format!("That was not JSON ({error})."))?;
    let reason = value
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    let steps = match value.get("action").and_then(Value::as_str) {
        Some("give_up") => return Ok(Guidance::GiveUp { reason }),
        // The screen is already past the failed step: nothing runs in its
        // place, and the flow goes on from the next step not covered.
        Some("skip") => Vec::new(),
        Some("retry") => {
            let steps: Vec<FlowStep> = value
                .get("steps")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|error| format!("Those steps are not flow steps ({error})."))?
                .unwrap_or_default();
            if steps.is_empty() || steps.len() > MAX_RESCUE_STEPS {
                return Err(format!(
                    "Give between 1 and {MAX_RESCUE_STEPS} steps, not {}.",
                    steps.len()
                ));
            }
            steps
        }
        _ => return Err("Set `action` to \"retry\", \"skip\", or \"give_up\".".to_owned()),
    };
    let failed_guards = briefing.flow.steps.get(briefing.failed).is_some_and(guards);
    if failed_guards && !steps.last().is_some_and(ends_in_guard) {
        return Err(
            "The failed step is a stop_before, which guards an irreversible action: \
             never skip it, and your steps must end in front of it with a stop_before too, \
             unconditionally — not only inside one branch of an `if`, and never only inside \
             a `repeat_until`, which can run zero times."
                .to_owned(),
        );
    }
    let covers = covered(&value, briefing)?;
    let flow = resumed(briefing, steps.clone(), covers);
    if flow.steps.is_empty() {
        return Err(
            "That leaves nothing to run: skip only to a step that is still to be done, \
             or give up."
                .to_owned(),
        );
    }
    let errors = crate::agentic::check_flow(&flow, &briefing.known, &briefing.secrets).errors;
    if errors.is_empty() {
        Ok(Guidance::Retry {
            reason,
            steps,
            covers,
        })
    } else {
        Err(format!(
            "Those steps are invalid:\n- {}",
            errors.join("\n- ")
        ))
    }
}

/// How many steps after the failed one the answer's `covers` drops, or why
/// it may not.
pub(super) fn covered(value: &Value, briefing: &Briefing) -> Result<usize, String> {
    let covers = value
        .get("covers")
        .and_then(Value::as_u64)
        .map_or(0, |covers| usize::try_from(covers).unwrap_or(usize::MAX));
    let rest = briefing
        .flow
        .steps
        .get(briefing.failed + 1..)
        .unwrap_or_default();
    if covers > rest.len() {
        return Err(format!(
            "`covers` is {covers}, but only {} steps follow the failed one.",
            rest.len()
        ));
    }
    if let Some(offset) = rest[..covers].iter().position(guards) {
        return Err(format!(
            "Step {} holds a stop_before, which guards an irreversible action: never cover it.",
            briefing.failed + offset + 2
        ));
    }
    Ok(covers)
}

/// Whether `step` holds a `stop_before`, at any depth. Used only to refuse
/// covering one (`covered`, above): a step that might hold a guard on some
/// path is never dropped, even when [`ends_in_guard`] would not credit it
/// with actually running one.
pub(super) fn guards(step: &FlowStep) -> bool {
    match step.action() {
        FlowAction::StopBefore(_) => true,
        FlowAction::If(branch) => branch.then.iter().chain(&branch.otherwise).any(guards),
        FlowAction::RepeatUntil(repeat) => repeat.steps.iter().any(guards),
        _ => false,
    }
}

/// Whether `step`, as the *last* step of a sequence, guarantees a
/// `stop_before` runs before anything after the sequence can: a bare
/// `stop_before`, or an `if` whose every branch is non-empty and itself ends
/// in one. `steps.iter().any(guards)` is not enough — a guard inside only
/// one branch of an `if`, or inside a `repeat_until` body that can run zero
/// times, can be skipped entirely, resuming the rest of the flow with no
/// checkpoint in front of the irreversible action it was meant to gate.
pub(super) fn ends_in_guard(step: &FlowStep) -> bool {
    match step.action() {
        FlowAction::StopBefore(_) => true,
        FlowAction::If(branch) => {
            !branch.then.is_empty()
                && !branch.otherwise.is_empty()
                && branch.then.last().is_some_and(ends_in_guard)
                && branch.otherwise.last().is_some_and(ends_in_guard)
        }
        // A `repeat_until` may run its body zero times, so even a body that
        // always ends in a guard cannot be credited with running one.
        _ => false,
    }
}

/// The flow that runs after a rescue: `guidance` in place of the failed step
/// and the `covers` steps after it, then every other step, unchanged.
#[must_use]
pub(crate) fn resumed(briefing: &Briefing, guidance: Vec<FlowStep>, covers: usize) -> Flow {
    let rest = briefing
        .flow
        .steps
        .get(briefing.failed + 1 + covers..)
        .unwrap_or_default();
    Flow {
        app: briefing.flow.app.clone(),
        vars: briefing.flow.vars.clone(),
        steps: guidance.into_iter().chain(rest.iter().cloned()).collect(),
    }
}
