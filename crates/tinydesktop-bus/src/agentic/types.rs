//! Wire types for native Jev-driven desktop control.

use serde::{Deserialize, Serialize};

/// Jev-compatible decision service selected by the host.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JevProvider {
    /// `TypeSafe`'s first-party System One API.
    #[default]
    TypeSafe,
    /// `OpenRouter`'s Jev-compatible decisions API.
    OpenRouter,
    /// Tiny Humans' authenticated `OpenRouter` proxy.
    TinyHumansOpenRouter,
}

/// Configures the Jev client retained by the loaded module.
///
/// This payload must be sent with `TinyBus` confidential delivery. Its custom
/// [`Debug`](std::fmt::Debug) implementation never prints the API key.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct JevConfig {
    api_key: String,
    /// Provider whose response contract should be validated.
    pub provider: JevProvider,
    /// Exact compatible endpoint, when the provider's conventional route is
    /// not desired.
    pub endpoint_url: Option<String>,
    /// Jev model or alias. Absent means `jev-latest`.
    pub model: Option<String>,
    /// Per-attempt HTTP timeout. Absent means the client default.
    pub timeout_ms: Option<u64>,
    /// Additional transient retries. Absent means the client default.
    pub max_retries: Option<u32>,
}

impl JevConfig {
    /// Builds a configuration carrying `api_key` and provider defaults.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            ..Self::default()
        }
    }

    /// Deliberately exposes the API key to the module constructing the client.
    #[must_use]
    pub fn api_key(&self) -> &str {
        &self.api_key
    }
}

impl std::fmt::Debug for JevConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JevConfig")
            .field("api_key", &"[REDACTED]")
            .field("provider", &self.provider)
            .field("endpoint_url", &self.endpoint_url)
            .field("model", &self.model)
            .field("timeout_ms", &self.timeout_ms)
            .field("max_retries", &self.max_retries)
            .finish()
    }
}

/// Resolves one natural-language intent against the current application.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResolveIntentRequest {
    /// Application whose current surface should be inspected.
    pub app: String,
    /// One action-oriented intent.
    pub intent: String,
    /// Caller-supplied text for a text-taking action.
    pub text: Option<String>,
    /// Optional container ref that narrows observation.
    pub root: Option<String>,
    /// Whether a safe resolved action should be executed.
    pub execute: bool,
    /// Whether ordinary field values may leave the machine for Jev.
    pub include_values: bool,
}

/// Runs a bounded observe-decide-act loop for one goal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RunGoalRequest {
    /// Application whose surfaces the loop controls.
    pub app: String,
    /// Visible end state the loop should reach.
    pub goal: String,
    /// Caller-supplied values consumed by text actions in order.
    pub text: Vec<String>,
    /// Optional container ref where observation starts.
    pub root: Option<String>,
    /// Whether ordinary field values may leave the machine for Jev.
    pub include_values: bool,
    /// Maximum executed actions, capped by the module at 40.
    pub max_steps: u32,
    /// Maximum Jev evaluations, capped by the module at 80.
    pub max_model_calls: u32,
    /// Failed or low-confidence turns tolerated before the loop gives up.
    /// Each one is recorded and fed back to Jev; capped by the module at 5.
    pub max_retries: u32,
    /// Whether observation starts from a shallow skeleton that Jev drills
    /// into, instead of the full tree.
    pub skeleton: bool,
}

impl Default for RunGoalRequest {
    fn default() -> Self {
        Self {
            app: String::new(),
            goal: String::new(),
            text: Vec::new(),
            root: None,
            include_values: false,
            max_steps: 40,
            max_model_calls: 80,
            max_retries: 2,
            skeleton: false,
        }
    }
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
    /// Scroll one container upward.
    ScrollUp,
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
    /// What the turn observed: elements that appeared, a failure, a retry.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
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
    /// Executed turns in order.
    pub turns: Vec<JevTurn>,
    /// Last decision when the loop stopped before executing it.
    pub pending: Option<JevDecision>,
    /// Provider measurements.
    pub metrics: JevMetrics,
}

/// Non-secret summary of the retained Jev client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JevConfiguration {
    /// Configured provider.
    pub provider: JevProvider,
    /// Requested model or alias.
    pub model: String,
    /// Exact endpoint override, when set.
    pub endpoint_url: Option<String>,
}
