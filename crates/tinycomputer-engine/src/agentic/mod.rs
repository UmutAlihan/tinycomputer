//! Native Jev-backed observation, intent resolution, and goal execution.
//!
//! `RunGoal` and `ResolveIntent` live here (see `README.md`); intent flows,
//! `RunFlow`, live in `flow/` and share only the Jev runtime and its error
//! mapping.
//!
//! - `runtime` holds [`JevRuntime`], its configuration, and the one door
//!   every Jev evaluation goes through;
//! - `resolve` makes the single decision `ResolveIntent` and every goal turn
//!   make, and gates it;
//! - `goal` is `RunGoal`'s entry point, `task` its bounded loop, and
//!   `pending` and `continuation` hold an action for confirmation and carry
//!   on once the caller answers;
//! - `reply` builds the envelopes and maps errors, and `backend` runs the
//!   desktop off the async runtime.

mod backend;
mod continuation;
mod flow;
mod goal;
mod journal;
mod pending;
mod policy;
mod reply;
mod resolve;
mod runtime;
mod sage;
mod screen;
mod task;
mod verify;

#[cfg(test)]
mod agentic_tests;

pub(crate) use flow::{check_flow, missing_inputs};
pub use flow::{flow_guide, run_flow, validate_flow};
pub use goal::run_goal;
pub use journal::{DEFAULT_DIR as JOURNAL_DEFAULT_DIR, JOURNAL_ENV, JOURNAL_FILE};
use reply::{internal_error, merge_metrics, provider_error, response};
pub use resolve::resolve_intent;
use runtime::Evaluator;
pub use runtime::JevRuntime;
