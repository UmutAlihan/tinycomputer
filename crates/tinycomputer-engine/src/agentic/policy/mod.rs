//! Jev request construction and deterministic execution gates.
//!
//! `request` builds what Jev is asked, `answer` reads what it said, and
//! `gate` decides, from the answer and the screen, whether it may act.

mod answer;
mod gate;
mod request;

pub(super) use answer::{choice, noul, parse_operation, target};
pub(super) use gate::{
    deterministic_destructive, exact_named_match, gate_with_evidence, playing_goal_satisfied,
    positional_match,
};
pub(super) use request::{ActionSpace, action_space, request, rerank_request, shortlist};

pub(super) const FLOOR: f64 = 0.55;
pub(super) const ACT: f64 = 0.70;
pub(super) const DESTRUCTIVE: f64 = 0.50;
const CORROBORATED_FLOOR: f64 = 0.45;
