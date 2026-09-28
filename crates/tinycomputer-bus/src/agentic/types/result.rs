//! What a goal run or an intent resolution reports back.

use super::VisiblePredicate;
use serde::{Deserialize, Serialize};

/// One predicate's compact, host-visible observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JevPredicateResult {
    /// Requested condition.
    pub predicate: VisiblePredicate,
    /// Whether the fresh observation satisfied it.
    pub matched: bool,
    /// Name of the observed element, when present.
    pub observed_name: Option<String>,
    /// Matched caller-supplied value or fragment; unrelated field content is omitted.
    pub observed_value: Option<String>,
    /// Observed state tokens only for a state predicate.
    pub observed_states: Vec<String>,
}

/// Last bounded accessibility evidence gathered by a goal run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JevObservation {
    /// Application reported by the snapshot.
    pub app: String,
    /// Window title reported by the snapshot.
    pub window: Option<String>,
    /// Surface type reported by the snapshot.
    pub surface: String,
    /// Independent predicate checks.
    pub predicates: Vec<JevPredicateResult>,
}

/// A closed operation Jev may select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JevOperation {
    /// Activate one element.
    Click,
    /// Put caller-supplied text into one element.
    TypeText,
    /// Put a toggle into its checked state.
    Check,
    /// Put a toggle into its unchecked state.
    Uncheck,
    /// Expand a disclosure.
    Expand,
    /// Collapse a disclosure.
    Collapse,
    /// Scroll one container downward.
    Scroll,
    /// Inspect one truncated container.
    Drill,
    /// Return observation to the full surface.
    Widen,
    /// Wait for the application to settle.
    Wait,
    /// The visible goal is satisfied.
    Done,
    /// No offered operation can make progress.
    Blocked,
}

/// What the module decided about one proposed step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JevDecisionKind {
    /// The step may be executed.
    Act,
    /// The step is destructive and requires explicit confirmation.
    ConfirmationRequired,
    /// The evidence did not clear the execution threshold.
    Abstain,
    /// The selected operation needs caller-supplied text.
    NeedsText,
    /// The goal is visibly complete.
    Done,
    /// No offered operation can make progress.
    Blocked,
}

/// Element selected for an operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JevTarget {
    /// Snapshot-qualified element ref.
    pub ref_id: String,
    /// Accessibility role.
    pub role: String,
    /// Accessible name or description.
    pub name: Option<String>,
}

/// Result of resolving one intent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JevDecision {
    /// Policy outcome.
    pub decision: JevDecisionKind,
    /// Selected closed operation.
    pub operation: JevOperation,
    /// Selected element, if the operation needs one.
    pub target: Option<JevTarget>,
    /// Concentration reported for the selected target or terminal operation.
    pub confidence: f64,
    /// Probability that the step is hard to undo.
    pub destructive: f64,
    /// Human-readable, secret-free policy explanation.
    pub reason: String,
    /// Whether the safe step was executed.
    pub executed: bool,
}

/// One executed goal-loop turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JevTurn {
    /// One-based executed step number.
    pub step: u32,
    /// Operation that ran.
    pub operation: JevOperation,
    /// Target used by the operation.
    pub target: Option<JevTarget>,
    /// Target confidence.
    pub confidence: f64,
    /// Whether the desktop command succeeded.
    pub ok: bool,
    /// Whether the observed surface changed afterwards.
    pub changed: bool,
}

/// Why a goal loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JevStopReason {
    /// Jev reported visible completion.
    Done,
    /// No offered operation could advance the goal.
    Blocked,
    /// A destructive step requires the host's confirmation.
    ConfirmationRequired,
    /// The host declined a pending action.
    Cancelled,
    /// The approved target no longer matched the observed desktop.
    StaleTarget,
    /// Confidence was too low to act.
    LowConfidence,
    /// No caller-supplied value remained for a text action.
    NeedsText,
    /// The action budget was reached.
    ActionBudget,
    /// The model-call budget was reached.
    ModelBudget,
    /// Three consecutive turns changed nothing.
    Stalled,
    /// A desktop command failed or had uncertain delivery.
    ActionFailed,
    /// Jev ended before the visible conditions were satisfied.
    VerificationFailed,
    /// The wall-clock budget was exhausted.
    TimeBudget,
    /// The observed app/window or chosen action left the caller's scope.
    ScopeChanged,
    /// A mutation may have been delivered; it must not be replayed blindly.
    ActionUncertain,
}

/// Aggregate provider measurements for one result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JevMetrics {
    /// Jev evaluations performed.
    pub calls: u32,
    /// HTTP attempts including retries.
    pub attempts: u32,
    /// Total provider latency in milliseconds.
    pub latency_ms: u64,
    /// Provider-reported input tokens.
    pub input_tokens: u64,
    /// Provider-reported output tokens.
    pub output_tokens: u64,
    /// Concrete model reported by the provider.
    pub model: Option<String>,
}

/// Result of a bounded goal loop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JevRunResult {
    /// Structured stop reason.
    pub stop: JevStopReason,
    /// True only when every requested success predicate was observed.
    pub verified: bool,
    /// Last compact observation, if one was obtained.
    pub final_observation: Option<JevObservation>,
    /// Executed turns in order.
    pub turns: Vec<JevTurn>,
    /// Last decision when the loop stopped before executing it.
    pub pending: Option<JevDecision>,
    /// One-use handle to approve or decline `pending` through `RunGoal`.
    pub confirmation_id: Option<String>,
    /// Provider measurements.
    pub metrics: JevMetrics,
}
