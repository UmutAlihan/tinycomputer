//! The catalogue entries for the members above the primitives: the one-shot
//! Jev flow members and the task members.

use crate::names::methods;

use super::{Family, Member, entry};

/// The flow members, in [`crate::names::METHODS`] order.
pub(super) const FLOW: &[Member] = &[
    entry(
        methods::RESOLVE_INTENT,
        Family::Flow,
        "Resolves one natural-language intent against the current screen.",
        true,
    ),
    entry(
        methods::RUN_GOAL,
        Family::Flow,
        "Runs a bounded Jev observe-decide-act loop toward a goal on the desktop.",
        true,
    ),
    entry(
        methods::RUN_FLOW,
        Family::Flow,
        "Runs a high-level intent flow on the desktop and returns per-step reports.",
        true,
    ),
    entry(
        methods::VALIDATE_FLOW,
        Family::Flow,
        "Checks a flow without touching the desktop or Jev.",
        false,
    ),
    entry(
        methods::FLOW_GUIDE,
        Family::Flow,
        "Returns the flow authoring guide as prompt text.",
        false,
    ),
];

/// The task members, in [`crate::names::METHODS`] order.
pub(super) const TASK: &[Member] = &[
    entry(
        methods::DESCRIBE,
        Family::Task,
        "Everything a caller needs: surfaces, the guide, schemas, examples, and this catalogue.",
        false,
    ),
    entry(
        methods::PLAN_TASK,
        Family::Task,
        "Drafts a flow for a plain-language task without acting; needs a planner.",
        false,
    ),
    entry(
        methods::START_TASK,
        Family::Task,
        "Starts a task from a flow or a plain-language goal and returns at once.",
        true,
    ),
    entry(
        methods::AWAIT_TASK,
        Family::Task,
        "Waits until a task needs something, finishes, or the timeout passes.",
        false,
    ),
    entry(
        methods::CONTINUE_TASK,
        Family::Task,
        "Answers a paused task — inputs, an approval, an answer — and resumes it.",
        true,
    ),
    entry(
        methods::CANCEL_TASK,
        Family::Task,
        "Stops a task and releases its browser session.",
        false,
    ),
    entry(
        methods::TASK_REPORT,
        Family::Task,
        "Everything a task did: steps, records, rescues, learned hints, and trace.",
        true,
    ),
    entry(
        methods::LIST_TASKS,
        Family::Task,
        "The tasks this module holds, newest first.",
        false,
    ),
];
