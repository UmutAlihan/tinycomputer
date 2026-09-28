//! What a flow run reports: why it stopped, and what each step did.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(doc)]
use super::RunFlowRequest;
use super::{FlowLoop, GroundingHint};
use crate::{JevMetrics, JevTarget};

/// Why a flow run stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowStopReason {
    /// Every step finished.
    Completed,
    /// A `stop_before` step found its irreversible action and stopped in
    /// front of it. Every earlier step finished.
    StoppedBeforeDestructive,
    /// A step could not be accomplished.
    StepFailed,
    /// The action budget ran out.
    ActionBudget,
    /// The Jev call budget ran out.
    ModelBudget,
    /// The flow did not validate.
    Invalid,
}

/// How one step ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepOutcome {
    /// The step was accomplished by acting.
    Done,
    /// The completion judge found the step already accomplished.
    AlreadyDone,
    /// The step's irreversible action was found and not performed.
    Gated,
    /// The step could not be accomplished.
    Failed,
}

/// One desktop action a step took.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowActionRecord {
    /// What was done: `click`, `fill`, `press cmd+n`, `undo`, `launch`, ….
    pub action: String,
    /// The element acted on, when there was one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<JevTarget>,
    /// Whether the desktop reported success.
    pub ok: bool,
    /// Detail: the delivery path, the error code, what changed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// What one step did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepReport {
    /// Position in the flow: `3`, or `4.2` for the second step nested in the
    /// fourth.
    pub path: String,
    /// The step kind as spelled on the wire.
    pub kind: String,
    /// The step's text, with variables substituted.
    pub text: String,
    /// How it ended.
    pub outcome: StepOutcome,
    /// Decision turns spent.
    pub turns: u32,
    /// Jev evaluations spent.
    pub jev_calls: u32,
    /// Desktop actions taken.
    pub actions: Vec<FlowActionRecord>,
    /// Decision loops that contributed.
    pub loops: Vec<FlowLoop>,
    /// Final completion or target confidence, when one was measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Why it ended the way it did.
    pub note: String,
}

/// Result of [`RunFlowRequest`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowRunResult {
    /// Why the run stopped.
    pub stop: FlowStopReason,
    /// One report per step reached, in order, nested steps included.
    pub steps: Vec<StepReport>,
    /// Variables at the end of the run, including those `read` steps set.
    pub vars: BTreeMap<String, String>,
    /// The irreversible action a `stop_before` step stopped in front of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<JevTarget>,
    /// Grounding hints learned this run, for the caller to pass back later.
    pub learned: Vec<GroundingHint>,
    /// Desktop actions taken.
    pub actions: u32,
    /// Provider measurements.
    pub metrics: JevMetrics,
    /// Every Jev exchange, when [`RunFlowRequest::trace`] asked for them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<JevExchange>,
}

/// One Jev request and its answers, as recorded by a traced run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JevExchange {
    /// The step that asked, as in [`StepReport::path`].
    pub step: String,
    /// The shared state the questions were asked against.
    pub state: Value,
    /// The questions, keyed by id.
    pub questions: Value,
    /// The answers, keyed by id.
    pub answers: Value,
}
