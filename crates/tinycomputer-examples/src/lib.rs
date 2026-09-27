//! Runnable examples and opt-in live verification for tinycomputer.
//!
//! The binaries in `src/bin` keep demonstration and release-verification code
//! out of the loadable module crate while remaining compiled by workspace CI.
//!
//! ```
//! use tinycomputer_bus::{JevProvider, RunGoalRequest};
//!
//! let request = RunGoalRequest {
//!     app: "Spotify".to_owned(),
//!     goal: "play the topmost liked song".to_owned(),
//!     ..RunGoalRequest::default()
//! };
//! assert_eq!(request.app, "Spotify");
//! assert_eq!(JevProvider::OpenRouter, JevProvider::OpenRouter);
//! ```

pub mod journal;
pub mod lab;

use tinycomputer_bus::FlowStrategy;

/// The flow strategy named by `TINYCOMPUTER_FLOW_STRATEGY` (`narrow` or
/// `wide`), for the live examples that compare the two; `None` when it is
/// unset or names neither.
#[must_use]
pub fn flow_strategy_from_env() -> Option<FlowStrategy> {
    parse_strategy(&std::env::var("TINYCOMPUTER_FLOW_STRATEGY").ok()?)
}

/// `narrow` or `wide` as a [`FlowStrategy`].
#[must_use]
pub fn parse_strategy(name: &str) -> Option<FlowStrategy> {
    serde_json::from_value(serde_json::Value::String(name.trim().to_owned())).ok()
}
