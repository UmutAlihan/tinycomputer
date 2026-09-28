//! The reply envelope every Agent member returns, and its error.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::browser::OutputRef;
use crate::flow::{Flow, FlowStep, GroundingHint, JevExchange, StepReport};

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
