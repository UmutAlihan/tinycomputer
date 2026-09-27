//! The tinycomputer agent runtime.
//!
//! This crate composes the surface adapters with Jev, `TypeSafe`'s decision
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
//! for the reasons `tinycomputer-desktop` documents. A [`JevRuntime`] is built
//! once from the module's private configuration and cloned per call.
//!
//! [`Tasks`] is the controller behind the Agent interface: it runs a task's
//! flow in the background and reports it as a status a model can act on,
//! pausing for missing values, irreversible actions, and always at payment.
//!
//! A [`Workspace`] joins the desktop and the browser into one surface, so a
//! flow's `browse` and `open` steps move it between a web page and an
//! application.
//!
//! The crate holds no bus: `tinycomputer` serves these functions over `TinyBus`.
//! The browser surface and the task controller arrive here next
//! (`docs/specs/unified-agent.md`).

mod agentic;
mod planner;
mod task;
mod workspace;

pub use agentic::{
    JOURNAL_DEFAULT_DIR, JOURNAL_ENV, JOURNAL_FILE, JevRuntime, flow_guide, resolve_intent,
    run_flow, run_goal, validate_flow,
};
pub use planner::{Completion, LanguageModel, Planner, REPAIRS, Role, Turn};
#[cfg(feature = "planner")]
pub use planner::{PLANNER_MODEL, PlannerConfig, open_router};
pub use task::{FlowFuture, FlowRunner, MAX_AWAIT_MS, MAX_TASKS, Tasks, TextFuture, capabilities};
pub use tinycomputer_bus::DesktopResponse;
use tinycomputer_desktop::Desktop;
pub use workspace::{BROWSER, Workspace};
