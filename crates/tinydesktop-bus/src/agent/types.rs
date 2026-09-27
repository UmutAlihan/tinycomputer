//! Task payloads for the Agent interface.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::browser::OutputRef;
use crate::flow::{Flow, GroundingHint, JevExchange, StepReport};

/// A task's identity, handed out by `StartTask`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(pub String);

impl TaskId {
    /// A task id from its string form.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Where a task may act.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceKind {
    /// Desktop applications, through the accessibility tree.
    Desktop,
    /// Web pages, through a browser session.
    Browser,
}

/// Every reply on the Agent interface: the value, or an error a caller can
/// act on. Never a transport failure for something the caller did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentResponse<T> {
    /// Whether the call did what was asked.
    pub ok: bool,
    /// The result, when `ok`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// Why not, when not `ok`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AgentError>,
}

impl<T> AgentResponse<T> {
    /// A successful reply carrying `data`.
    #[must_use]
    pub fn ok(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    /// A failed reply carrying `error`.
    #[must_use]
    pub fn err(error: AgentError) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(error),
        }
    }
}

/// A failed call, phrased for a model to act on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentError {
    /// A stable, `SCREAMING_SNAKE_CASE` code, such as `NO_SUCH_TASK`.
    pub code: String,
    /// What went wrong, in one sentence.
    pub message: String,
    /// What to do about it, in one sentence.
    pub hint: String,
    /// Whether retrying — possibly with the hint applied — can succeed.
    pub recoverable: bool,
}

impl AgentError {
    /// An error with a code, message, and hint.
    #[must_use]
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
        recoverable: bool,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hint: hint.into(),
            recoverable,
        }
    }
}

/// `StartTask`: what to accomplish, with what, and within what limits.
///
/// Give either `task` (plain language; needs the planner configured, or the
/// task pauses with `needs_plan`) or `flow` (a high-level flow the caller
/// wrote from `Describe`'s guide). Both together means "run this flow; the
/// task text explains it".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StartTaskRequest {
    /// The goal in plain language.
    pub task: Option<String>,
    /// A flow to run instead of planning one.
    pub flow: Option<Flow>,
    /// Values the task may type, by name — traveller name, email, phone. They
    /// stay on this machine: models see the names only. Card data is refused.
    pub facts: BTreeMap<String, String>,
    /// Where the task may act and what it may commit to.
    pub constraints: TaskConstraints,
    /// Upper bounds on the work a task may do.
    pub budget: TaskBudget,
    /// Grounding hints from an earlier run, so this one reads less.
    pub memory: Vec<GroundingHint>,
    /// Record every Jev exchange for `TaskReport`.
    pub trace: bool,
}

/// Where a task may act and what it may commit to.
///
/// A payment is always a checkpoint and cannot be allowed here: the task
/// stops on the payment page and hands it back.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskConstraints {
    /// Surfaces the task may use; empty means every available one.
    pub surfaces: Vec<SurfaceKind>,
    /// Origins browser sessions may load, such as `https://.makemytrip.com`
    /// for a site and its subdomains; empty means any.
    pub origins: Vec<String>,
    /// Perform irreversible actions (send, delete, confirm a booking) without
    /// pausing for approval.
    pub allow_destructive: bool,
    /// Attach the task's browser work to an existing browser at this `DevTools`
    /// endpoint, such as the user's own signed-in Chrome.
    pub browser_endpoint: Option<String>,
    /// Show the browser rather than running it headless.
    pub headed: bool,
}

/// Upper bounds on a task. Unset fields take the module's defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskBudget {
    /// Actions across every surface.
    pub max_actions: Option<u32>,
    /// Jev decisions.
    pub max_model_calls: Option<u32>,
    /// Wall-clock time, excluding time spent waiting for the caller.
    pub max_elapsed_ms: Option<u64>,
}

/// `AwaitTask`: wait for a task to need something or finish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwaitTaskRequest {
    /// The task.
    pub id: TaskId,
    /// The longest to wait; a view comes back sooner when the task changes
    /// state. Capped by the module.
    #[serde(default = "default_await_ms")]
    pub timeout_ms: u64,
}

const fn default_await_ms() -> u64 {
    30_000
}

/// `ContinueTask`: answer what a paused task asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContinueTaskRequest {
    /// The task.
    pub id: TaskId,
    /// Values for a `needs_input` pause, by the field names it listed. Added
    /// to the task's facts.
    pub inputs: BTreeMap<String, String>,
    /// The decision on a `needs_approval` pause, or `true` to go past a
    /// non-payment `checkpoint`.
    pub approve: Option<bool>,
    /// A free-text answer, for a pause that asked a question — or, for
    /// `needs_human`, `"done"` once the person has finished.
    pub answer: Option<String>,
}

/// A task named in a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRef {
    /// The task.
    pub id: TaskId,
}

/// `PlanTask`: draft a flow without acting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanTaskRequest {
    /// The goal in plain language.
    pub task: String,
    /// The names of the facts the caller can supply — values are not needed
    /// to plan.
    pub fact_names: Vec<String>,
    /// Surfaces to plan for; empty means every available one.
    pub surfaces: Vec<SurfaceKind>,
}

/// A drafted plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskPlan {
    /// The flow the task would run.
    pub flow: Flow,
    /// Facts the flow uses that the caller did not name, to collect first.
    pub questions: Vec<InputField>,
    /// Assumptions the planner made, in plain words.
    pub notes: Vec<String>,
}

/// A task's current state, as every call reports it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskView {
    /// The task.
    pub id: TaskId,
    /// Where it is, and what it needs.
    pub status: TaskStatus,
    /// One sentence on what is happening, for a model or a person.
    pub summary: String,
    /// The step in progress or paused on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<StepView>,
    /// Steps finished over steps planned, from 0 to 1.
    pub progress: f32,
    /// The member calls that make sense now, such as `AwaitTask` or
    /// `ContinueTask`.
    pub next: Vec<String>,
}

/// The step a task is on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepView {
    /// Its position, from zero.
    pub index: usize,
    /// How many top-level steps the flow has.
    pub total: usize,
    /// Its kind, such as `browse` or `enter`.
    pub kind: String,
    /// What it is trying to do.
    pub intent: String,
    /// The surface it runs on, such as `browser` or `Mail`.
    pub surface: String,
}

/// Where a task is, and what it needs from the caller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TaskStatus {
    /// Working. Call `AwaitTask`.
    Running,
    /// Paused for values it does not have. Answer with `ContinueTask.inputs`.
    NeedsInput {
        /// What to supply.
        fields: Vec<InputField>,
    },
    /// Paused before an irreversible action. Answer with
    /// `ContinueTask.approve`.
    NeedsApproval {
        /// What it would do, in plain words.
        action: String,
        /// The control it would press.
        target: String,
        /// The screen as it stands, for a person to check.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        screenshot: Option<OutputRef>,
    },
    /// Stopped at a checkpoint — always on reaching a payment page. The task
    /// hands over here; for a payment it cannot be continued.
    Checkpoint {
        /// Why it stopped.
        reason: String,
        /// The page or window it stopped on.
        location: String,
        /// The screen as it stands.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        screenshot: Option<OutputRef>,
        /// What has been done so far, in plain words.
        summary: String,
        /// Whether `ContinueTask.approve` may take it further.
        continuable: bool,
    },
    /// Blocked on something only a person can do: a captcha, a login, a
    /// one-time code. Answer `ContinueTask.answer = "done"` once it is done.
    NeedsHuman {
        /// What the person must do.
        reason: String,
        /// The screen as it stands.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        screenshot: Option<OutputRef>,
    },
    /// A plain-language task arrived and no planner is configured. Write a
    /// flow from `guide` and start again with `StartTask.flow`.
    NeedsPlan {
        /// The flow authoring guide.
        guide: String,
    },
    /// Finished.
    Done {
        /// The outcome, in plain words.
        answer: String,
        /// What `extract` and `pick` steps collected, by variable name.
        records: BTreeMap<String, Vec<BTreeMap<String, String>>>,
    },
    /// Could not finish.
    Failed {
        /// The step that failed, from zero.
        step: Option<usize>,
        /// Why.
        reason: String,
        /// What a caller could change to succeed.
        hint: String,
        /// Whether starting again, changed per the hint, can succeed.
        recoverable: bool,
    },
    /// Stopped by `CancelTask`.
    Cancelled,
}

impl TaskStatus {
    /// Whether the task is over and will not change again.
    #[must_use]
    pub fn is_final(&self) -> bool {
        matches!(
            self,
            Self::Done { .. }
                | Self::Failed { .. }
                | Self::Cancelled
                | Self::Checkpoint {
                    continuable: false,
                    ..
                }
        )
    }
}

/// A value a paused task needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputField {
    /// The name to answer under, in `ContinueTask.inputs`.
    pub name: String,
    /// Why it is needed, in plain words.
    pub why: String,
    /// What kind of value it is.
    pub kind: InputKind,
    /// The allowed values, for a choice.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

/// The kind of value an input field takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    /// Free text.
    Text,
    /// A calendar date, as `YYYY-MM-DD`.
    Date,
    /// An email address.
    Email,
    /// A phone number, with country code.
    Phone,
    /// A number.
    Number,
    /// One of `options`.
    Choice,
}

/// `TaskReport`: everything a task did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskReport {
    /// The task as it stands.
    pub view: TaskView,
    /// The flow it ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<Flow>,
    /// One report per step run.
    pub steps: Vec<StepReport>,
    /// What `extract` and `pick` steps collected, by variable name.
    pub records: BTreeMap<String, Vec<BTreeMap<String, String>>>,
    /// Screenshots taken at checkpoints, approvals, and failures.
    pub artifacts: Vec<OutputRef>,
    /// Grounding hints learned, to pass as `StartTask.memory` next time.
    pub learned: Vec<GroundingHint>,
    /// Every Jev exchange, when `StartTask.trace` was set.
    pub trace: Vec<JevExchange>,
}

/// `Describe`: how to use this module, in one reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    /// The contract version the module serves.
    pub contract_version: (u32, u32),
    /// Each surface and whether it is usable now.
    pub surfaces: Vec<SurfaceAvailability>,
    /// Whether Jev is configured; without it no task can run.
    pub jev_configured: bool,
    /// Whether the planner is configured; without it `task` needs a `flow`.
    pub planner_configured: bool,
    /// The flow step kinds.
    pub step_kinds: Vec<String>,
    /// The flow authoring guide.
    pub guide: String,
    /// Each member, with its input and output JSON Schemas.
    pub members: Vec<MemberDoc>,
    /// Worked requests, ready to adapt.
    pub examples: Vec<Example>,
}

/// Whether a surface is usable, and why not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceAvailability {
    /// The surface.
    pub kind: SurfaceKind,
    /// Whether tasks can use it now.
    pub available: bool,
    /// What is missing, when not available — a permission, a browser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One member, documented for a model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberDoc {
    /// The member name.
    pub name: String,
    /// What it does, in one sentence.
    pub summary: String,
    /// Whether frames to it must be delivered confidentially.
    pub confidential: bool,
    /// JSON Schema of its argument.
    pub input: Value,
    /// JSON Schema of its reply's `data`.
    pub output: Value,
}

/// A worked request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Example {
    /// What it shows.
    pub title: String,
    /// The member it calls.
    pub member: String,
    /// The argument to send.
    pub request: Value,
}
