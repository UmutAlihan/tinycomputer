//! High-level intent flows: app-agnostic scripts grounded by Jev decision loops.
//!
//! A caller — often a language model that has never seen the application —
//! writes a [`Flow`] of plain steps ("start a new email message"). The module
//! grounds every step on the live screen with many small Jev questions, so the
//! flow itself carries no UI knowledge. [`FLOW_GUIDE`] is the authoring guide,
//! written to be pasted into a model's prompt.

mod types;

pub use types::{
    ChooseStep, Flow, FlowAction, FlowActionRecord, FlowLoop, FlowRunResult, FlowStep,
    FlowStopReason, FlowValidation, GroundingHint, IfStep, JevExchange, PickStep, ReadStep, RepeatStep,
    RunFlowRequest, STEP_KINDS, Slot, Slots, StepOutcome, StepReport, ValidateFlowRequest,
};

/// How to write a flow: the grammar, rules of thumb, and worked examples.
///
/// Returned verbatim by the `FlowGuide` member so a host can hand it to the
/// model that authors flows. Every JSON example in it parses as a [`Flow`].
pub const FLOW_GUIDE: &str = include_str!("guide.md");

#[cfg(test)]
mod test;
