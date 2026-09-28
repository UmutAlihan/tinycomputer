//! Wire types for native Jev-driven desktop control.

mod config;
mod request;
mod result;

pub use config::{JevConfig, JevConfiguration, JevProvider};
pub use request::{GoalContinuation, ResolveIntentRequest, RunGoalRequest, VisiblePredicate};
pub use result::{
    JevDecision, JevDecisionKind, JevMetrics, JevObservation, JevOperation, JevPredicateResult,
    JevRunResult, JevStopReason, JevTarget, JevTurn,
};
