//! What a caller asks Jev to do: resolve an intent, or run a goal.

use std::collections::BTreeMap;

use super::JevOperation;
use serde::{Deserialize, Serialize};

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
    /// Exact window title to retain throughout the task.
    pub window: Option<String>,
    /// Exact window ID from `ListWindows`; binds every task observation.
    pub window_id: Option<String>,
    /// Allowed mutating operations. Empty keeps the legacy operation set.
    pub allowed_operations: Vec<JevOperation>,
    /// Exact accessible names or descriptions of permitted action targets.
    /// Empty keeps the legacy target set.
    pub allowed_targets: Vec<String>,
    /// Prepared text keyed by the accessible field name or description.
    pub text_slots: BTreeMap<String, String>,
    /// Accessibility-visible predicates that must all hold for verified completion.
    /// Empty preserves legacy Jev completion behavior.
    pub success: Vec<VisiblePredicate>,
    /// Whether ordinary field values may leave the machine for Jev.
    pub include_values: bool,
    /// Maximum executed actions, capped by the module at 40.
    pub max_steps: u32,
    /// Maximum Jev evaluations, capped by the module at 80.
    pub max_model_calls: u32,
    /// Whole-task wall-clock budget in milliseconds, capped at five minutes.
    pub max_elapsed_ms: u64,
    /// Whether consequential actions require a separate confirmation call.
    pub require_confirmations: bool,
    /// One-use handle from a previous confirmation stop. Other fields are ignored on continuation.
    pub continuation: Option<GoalContinuation>,
}

impl Default for RunGoalRequest {
    fn default() -> Self {
        Self {
            app: String::new(),
            goal: String::new(),
            text: Vec::new(),
            root: None,
            window: None,
            window_id: None,
            allowed_operations: Vec::new(),
            allowed_targets: Vec::new(),
            text_slots: BTreeMap::new(),
            success: Vec::new(),
            include_values: false,
            max_steps: 40,
            max_model_calls: 80,
            max_elapsed_ms: 120_000,
            require_confirmations: true,
            continuation: None,
        }
    }
}

/// A deterministic condition checked against a fresh accessibility snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VisiblePredicate {
    /// An element with this exact accessible name or description exists.
    NamePresent {
        /// Exact accessible name or description.
        name: String,
    },
    /// A descendant of an exactly named container has a name containing this text.
    NameContains {
        /// Required fragment of the descendant's accessible name.
        fragment: String,
        /// Exact accessible name of an ancestor container.
        within: String,
    },
    /// A named element holds this exact string value.
    ValueEquals {
        /// Exact accessible name or description.
        name: String,
        /// Expected complete string value.
        value: String,
    },
    /// A named element's string value contains this caller-supplied fragment.
    ValueContains {
        /// Exact accessible name or description.
        name: String,
        /// Expected string fragment.
        value: String,
    },
    /// A named element exposes this state token.
    StateContains {
        /// Exact accessible name or description.
        name: String,
        /// Expected accessibility state token.
        state: String,
    },
}

/// Host response to a pending consequential desktop action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalContinuation {
    /// Opaque one-use handle returned by the module.
    pub id: String,
    /// Whether a person approved the exact pending operation and target.
    pub approve: bool,
}
