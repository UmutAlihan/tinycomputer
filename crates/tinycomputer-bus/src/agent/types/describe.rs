//! The `Describe` reply: capabilities, surfaces, and member docs for a model.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{LanguageModelConfiguration, SurfaceKind};
use crate::JevConfiguration;

/// `Describe`: how to use this module, in one reply.
// Each `*_configured` flag is an independent fact a model reads by name;
// folding them into one enum or bitset would change the wire form, a major
// bump, for a tidiness the reply's readers gain nothing from.
#[allow(clippy::struct_excessive_bools)]
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
    /// Whether a failed step is handed to a reasoning model for guidance
    /// before the task fails.
    #[serde(default)]
    pub rescue_configured: bool,
    /// Whether a task may ask for its answer in a shape (`output`).
    #[serde(default)]
    pub output_configured: bool,
    /// The decision model the loops ask — its provider, model, and endpoint
    /// override — when one is configured (contract 2.8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_model: Option<JevConfiguration>,
    /// The planner's route and model, when a planner is configured (2.8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planner_model: Option<LanguageModelConfiguration>,
    /// The rescuer's route and model, when rescues are configured (2.8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rescue_model: Option<LanguageModelConfiguration>,
    /// The shaper's route and model, when output shapes are configured (2.8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_model: Option<LanguageModelConfiguration>,
    /// The flow step kinds.
    pub step_kinds: Vec<String>,
    /// The flow authoring guide.
    pub guide: String,
    /// Each member, with its input and output JSON Schemas.
    pub members: Vec<MemberDoc>,
    /// Worked requests, ready to adapt.
    pub examples: Vec<Example>,
    /// Every member the module serves — task, flow, desktop, and browser —
    /// with its family and a one-line summary, so a caller knows the
    /// primitives exist without reading the contract.
    #[serde(default)]
    pub catalogue: Vec<crate::catalogue::MemberSummary>,
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
