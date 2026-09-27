//! The tinydesktop agent runtime.
//!
//! This crate composes the surface adapters with Jev, TypeSafe's decision
//! model, into the loops a caller drives through the module:
//!
//! - [`resolve_intent`] grounds one described element and optionally acts on
//!   it;
//! - [`run_goal`] runs a bounded, scoped goal with visible success predicates;
//! - [`run_flow`] runs a high-level, UI-agnostic intent flow, grounding each
//!   step on the live screen with small Jev questions
//!   (`docs/specs/jev-intent-flows.md`);
//! - [`validate_flow`] and [`flow_guide`] check and document flows without
//!   touching the desktop.
//!
//! Every entry point returns a [`DesktopResponse`] envelope, never a `Result`,
//! for the reasons `tinydesktop-desktop` documents. A [`JevRuntime`] is built
//! once from the module's private configuration and cloned per call.
//!
//! The crate holds no bus: `tinydesktop` serves these functions over `TinyBus`.
//! The browser surface and the task controller arrive here next
//! (`docs/specs/unified-agent.md`).

mod agentic;

pub use agentic::{JevRuntime, flow_guide, resolve_intent, run_flow, run_goal, validate_flow};
pub use tinydesktop_bus::DesktopResponse;
use tinydesktop_desktop::Desktop;
