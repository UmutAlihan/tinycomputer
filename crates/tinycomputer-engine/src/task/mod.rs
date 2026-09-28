//! The task controller behind the Agent interface.
//!
//! [`Tasks`] takes a task — a flow and the facts it may type — runs it in
//! the background, and reports it as a [`TaskView`] a model can act on. It
//! pauses only for what the caller must decide:
//!
//! - **missing values** — a `${name}` the flow uses and no fact supplies
//!   becomes `needs_input` before anything runs;
//! - **irreversible actions** — a `stop_before` the task may not perform
//!   becomes `needs_approval`, and approving it performs that action and
//!   carries on with the steps after it;
//! - **payment** — the control that pays is never pressed on its own: by
//!   default reaching it is a final checkpoint, and under
//!   [`PaymentMode::FillThenApprove`] the payment form is filled and pressing
//!   it waits for `needs_approval`.
//!
//! - **a failed step** — when a rescuer is configured, a step that fails is
//!   first handed to it ([`crate::Rescuer`]), up to five times a task: its
//!   steps run in place of the failed one and the task carries on. Only when
//!   it gives up, or the rescues are spent, does the task fail.
//!
//! Facts reach the flow as variables, so they are typed locally. Shared ones
//! also brief Jev by value ([`FlowBrief`]); secret ones reach Jev only as
//! `${name}`. The flow runs with `include_values` on, so Jev reads what a
//! field holds and can check what was typed; the flow runtime masks every
//! secret value, there too, before anything reaches Jev. Every summary is
//! redacted of every fact.
//!
//! How a flow actually runs is behind [`FlowRunner`], so this controller is
//! tested with scripted runs and the module plugs in the real flow runtime.
//!
//! The controller's API is in `controller`; each task's cell and state in
//! `store`; the background run in `drive`, capped by `budget` and briefed by
//! `brief`; answering a paused task in `resume`; rescuing a failed step in
//! `recovery`; spotting a wall only a person can pass in `human`; and what
//! the caller sees in `publish`; the screenshots a stopped run leaves in
//! `artifact`.

mod artifact;
mod brief;
mod budget;
mod controller;
mod describe;
mod drive;
mod errors;
mod human;
mod interpret;
mod names;
mod publish;
mod recovery;
mod resume;
mod store;

use std::future::Future;
use std::pin::Pin;

use tinycomputer_bus::agent::{TaskConstraints, TaskId};
use tinycomputer_bus::browser::OutputRef;
use tinycomputer_bus::{DesktopResponse, RunFlowRequest};

pub use controller::Tasks;
pub use describe::capabilities;
pub(crate) use names::input_kind;

/// The future a [`FlowRunner`] returns: the flow runtime's reply envelope.
pub type FlowFuture = Pin<Box<dyn Future<Output = DesktopResponse> + Send>>;

/// The future [`FlowRunner::visible_text`] returns.
pub type TextFuture = Pin<Box<dyn Future<Output = Vec<String>> + Send>>;

/// The future [`FlowRunner::capture`] returns: a held screenshot, if the
/// task's surface could take one.
pub type CaptureFuture = Pin<Box<dyn Future<Output = Option<OutputRef>> + Send>>;

/// Runs a task's flows, on surfaces that live as long as the task.
pub trait FlowRunner: Send + Sync + 'static {
    /// Runs `request` for `task` within `constraints`, returning `RunFlow`'s
    /// reply. Runs of one task share its surfaces, so a continuation picks up
    /// on the page or window the last run left.
    fn run(
        &self,
        task: &TaskId,
        constraints: &TaskConstraints,
        request: RunFlowRequest,
    ) -> FlowFuture;

    /// The visible text of the task's surface now, to spot a wall only a
    /// person can pass. Empty by default.
    fn visible_text(&self, _task: &TaskId) -> TextFuture {
        Box::pin(async { Vec::new() })
    }

    /// A screenshot of the task's surface as it stands, held for the caller
    /// to read. Taken whenever a run stops on its own — paused, finished,
    /// failed, or cut off by its time budget — before the task's surfaces
    /// are released, so a finished or failed task still leaves one.
    /// `CancelTask` releases at once without one: the caller chose to stop,
    /// and can take its own with `BrowserScreenshot` first. `None` by
    /// default, and whenever the surface cannot take one.
    fn capture(&self, _task: &TaskId) -> CaptureFuture {
        Box::pin(async { None })
    }

    /// Lets go of whatever the task held, once it has ended.
    fn release(&self, _task: &TaskId) {}
}

/// How many tasks the controller holds; finished ones are dropped first.
pub const MAX_TASKS: usize = 32;

/// The longest a single `AwaitTask` waits.
pub const MAX_AWAIT_MS: u64 = 60_000;

/// Jev evaluations a task may spend when its budget does not say. Jev is
/// cheap, so this is generous: every decision is voted on several ways.
pub(crate) const DEFAULT_MODEL_CALLS: u32 = 6000;

/// How many ways each decision is asked when a task's budget does not say.
pub(crate) const DEFAULT_VOTES: u32 = 7;

/// The longest one rescue may think before the task fails without it.
pub(crate) const RESCUE_TIMEOUT_MS: u64 = 120_000;

/// The longest the shaping pass may take before the task fails without its
/// result.
pub(crate) const SHAPE_TIMEOUT_MS: u64 = 120_000;

#[cfg(test)]
mod task_tests;
