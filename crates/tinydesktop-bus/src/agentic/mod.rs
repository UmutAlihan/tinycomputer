//! Jev configuration, one-step resolution, and goal-running payloads.

mod types;

pub use types::{
    GoalContinuation, JevConfig, JevConfiguration, JevDecision, JevDecisionKind, JevMetrics,
    JevObservation, JevOperation, JevPredicateResult, JevProvider, JevRunResult, JevStopReason,
    JevTarget, JevTurn, ResolveIntentRequest, RunGoalRequest, VisiblePredicate,
};

#[cfg(test)]
mod test;
