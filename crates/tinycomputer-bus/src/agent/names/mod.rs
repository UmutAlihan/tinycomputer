//! The Agent members' bus identity: where they are served, and one constant
//! per member, in dispatch order.
//!
//! A `TinyBus` module exports one interface at one object path, so the Agent
//! members are served on the module's interface beside the desktop members
//! (their names do not collide). [`INTERFACE`] and [`OBJECT_PATH`] are that
//! interface's, and the members appear in [`crate::names::METHODS`] too.

/// The interface the Agent members are served on.
pub const INTERFACE: &str = crate::names::INTERFACE;

/// The object path the Agent members are served at.
pub const OBJECT_PATH: &str = crate::names::OBJECT_PATH;

/// One constant per member of [`INTERFACE`].
pub mod methods {
    /// Everything a caller needs to use this interface, in one call.
    ///
    /// Takes nothing and returns a [`crate::agent::Capabilities`]: the
    /// surfaces available, whether Jev and the planner are configured, the
    /// flow guide, every member's input and output JSON Schema, and worked
    /// examples.
    pub const DESCRIBE: &str = "Describe";

    /// Drafts a flow for a task without touching anything.
    ///
    /// Takes a [`crate::agent::PlanTaskRequest`] and returns a
    /// [`crate::agent::AgentResponse`] of [`crate::agent::TaskPlan`].
    pub const PLAN_TASK: &str = "PlanTask";

    /// Starts a task and returns at once with its first view.
    ///
    /// Takes a [`crate::agent::StartTaskRequest`] and returns a
    /// [`crate::agent::AgentResponse`] of [`crate::agent::TaskView`].
    /// Confidential: the request carries the caller's facts.
    pub const START_TASK: &str = "StartTask";

    /// Waits until a task needs something, finishes, or `timeout_ms` passes.
    ///
    /// Takes a [`crate::agent::AwaitTaskRequest`] and returns a
    /// [`crate::agent::AgentResponse`] of [`crate::agent::TaskView`].
    pub const AWAIT_TASK: &str = "AwaitTask";

    /// Answers what a paused task asked for — inputs, an approval, an
    /// answer — and resumes it.
    ///
    /// Takes a [`crate::agent::ContinueTaskRequest`] and returns a
    /// [`crate::agent::AgentResponse`] of [`crate::agent::TaskView`].
    /// Confidential: inputs are the caller's facts.
    pub const CONTINUE_TASK: &str = "ContinueTask";

    /// Stops a task. Cancelling one already finished succeeds.
    ///
    /// Takes a [`crate::agent::TaskRef`] and returns a
    /// [`crate::agent::AgentResponse`] of [`crate::agent::TaskView`].
    pub const CANCEL_TASK: &str = "CancelTask";

    /// The full record of a task: steps, extracted records, artifacts, and
    /// (when requested at start) every Jev exchange.
    ///
    /// Takes a [`crate::agent::TaskReportRequest`] and returns a
    /// [`crate::agent::AgentResponse`] of [`crate::agent::TaskReport`].
    /// Confidential: it carries page data.
    pub const TASK_REPORT: &str = "TaskReport";

    /// The tasks this module holds, newest first.
    ///
    /// Takes nothing and returns a [`crate::agent::AgentResponse`] of
    /// `Vec<`[`crate::agent::TaskView`]`>`.
    pub const LIST_TASKS: &str = "ListTasks";
}

/// Every member of [`INTERFACE`], in dispatch order.
pub const METHODS: &[&str] = &[
    methods::DESCRIBE,
    methods::PLAN_TASK,
    methods::START_TASK,
    methods::AWAIT_TASK,
    methods::CONTINUE_TASK,
    methods::CANCEL_TASK,
    methods::TASK_REPORT,
    methods::LIST_TASKS,
];

/// The members whose frames carry facts or page data, and so require
/// confidential delivery to an attested module.
pub const CONFIDENTIAL: &[&str] = &[
    methods::START_TASK,
    methods::CONTINUE_TASK,
    methods::TASK_REPORT,
];

#[cfg(test)]
mod names_tests;
