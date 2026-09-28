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

pub mod host;
pub mod journal;
pub mod lab;
pub mod task;

use tinycomputer_browser::Perception;
use tinycomputer_bus::{Deliberation, FlowStrategy};

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

/// The deliberation level named by `TINYCOMPUTER_FLOW_DELIBERATION` (`off`,
/// `standard`, or `deep`), for the live examples that compare them; `None`
/// when it is unset or names none of them, which runs the module's default.
#[must_use]
pub fn flow_deliberation_from_env() -> Option<Deliberation> {
    parse_deliberation(&std::env::var("TINYCOMPUTER_FLOW_DELIBERATION").ok()?)
}

/// `off`, `standard`, or `deep` as a [`Deliberation`].
///
/// ```
/// use tinycomputer_bus::Deliberation;
/// use tinycomputer_examples::parse_deliberation;
///
/// assert_eq!(parse_deliberation(" off "), Some(Deliberation::Off));
/// assert_eq!(parse_deliberation("deep"), Some(Deliberation::Deep));
/// assert_eq!(parse_deliberation("maximum"), None);
/// ```
#[must_use]
pub fn parse_deliberation(name: &str) -> Option<Deliberation> {
    serde_json::from_value(serde_json::Value::String(name.trim().to_owned())).ok()
}

/// How the live browser examples read a page, from
/// `TINYCOMPUTER_BROWSER_PERCEPTION`: `tree` reads the accessibility tree
/// alone; anything else, or nothing, reads by sight (the default).
#[must_use]
pub fn perception_from_env() -> Perception {
    parse_perception(&std::env::var("TINYCOMPUTER_BROWSER_PERCEPTION").unwrap_or_default())
}

/// `tree` as [`Perception::Tree`]; anything else as [`Perception::Sight`].
///
/// ```
/// use tinycomputer_browser::Perception;
/// use tinycomputer_examples::parse_perception;
///
/// assert_eq!(parse_perception(" Tree "), Perception::Tree);
/// assert_eq!(parse_perception(""), Perception::Sight);
/// ```
#[must_use]
pub fn parse_perception(name: &str) -> Perception {
    if name.trim().eq_ignore_ascii_case("tree") {
        Perception::Tree
    } else {
        Perception::Sight
    }
}
