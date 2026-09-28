//! The escalation ladder: what a deliberating decision asks before it acts.
//!
//! When the evidence behind an answer is thin (`evidence.rs`), the decision
//! climbs, one rung at a time, and stops at the first rung that settles it:
//!
//! 1. **More framings** (`widen`): the same request asked the ways it was
//!    not yet asked, up to [`vote::MAX_VOTES`], every answer joining the
//!    question's ballot.
//! 2. **A duel** (`duel.rs`): the finalists of a target Choice compared two
//!    at a time, in both orders.
//! 3. **Contrast** (deep only, when the duel named no champion): the two
//!    leaders each asked "is this the element?" beside "is this only
//!    something similar or next to it?".
//!
//! A close call no rung settles is still acted on, at its best ranking, with
//! the runners-up kept for a backtrack: deliberation changes picks, and
//! refuses one only when the evidence says nothing on screen serves.
//! 4. **Views** (deep only, a judgement that would pass): a yes/no asked again over other
//!    renderings of the screen — the screen alone, without the history that
//!    can lead it, and what changed since the step began. The readings are
//!    combined by their median, so one dissenting view neither passes nor
//!    vetoes a judgement: the screen alone cannot show that a radio already
//!    selected was selected by this step, and must not overrule the views
//!    that can.
//!
//! Every rung is one `FlowRun::ask`, so the budget, masking, voting, and
//! journal all apply, and every rung first checks the budget has room: a
//! run short of calls stops climbing and decides with what it has, rather
//! than failing for lack of deliberation.

mod belief;
mod target;

use std::collections::BTreeMap;

use serde_json::json;
use tinycomputer_bus::FlowLoop;
use tinyinference_decisions::{Answer, EvaluationRequest};

use super::{
    AgentBackend, FlowRun, StepLog,
    ask::{self},
    evidence::{Bar, Evidence, Verdict},
    view::{Candidate, Screen},
};

/// Least calibrated belief a finalist needs, with no champion, to be taken.
pub(super) const CONTRAST_ACCEPT: f64 = 0.65;
/// Least lead that finalist needs over the other one contrasted.
pub(super) const CONTRAST_LEAD: f64 = 0.2;
/// Most runners-up a grounding keeps for a backtrack.
const MAX_FRONTIER: usize = 6;

/// A yes/no judgement a deliberating decision reads: `yes` calibrated
/// against `no`, combined with `top`'s highest level when one is asked.
#[derive(Debug, Clone, Copy)]
pub(super) struct Belief<'a> {
    /// Where it is decided, for the journal: `done`, `holds`.
    pub(super) site: &'static str,
    /// The positive Noul's id.
    pub(super) yes: &'a str,
    /// The negated Noul's id.
    pub(super) no: &'a str,
    /// The Score whose top level joins the belief, if any.
    pub(super) top: Option<&'a str>,
    /// The threshold the belief is judged against.
    pub(super) threshold: f64,
}

impl Belief<'_> {
    /// The belief `answers` hold.
    pub(super) fn read(&self, answers: &BTreeMap<String, Answer>) -> Option<f64> {
        let calibrated = ask::calibrated(answers, self.yes, self.no);
        match self.top {
            Some(top) => ask::combined(calibrated, ask::top_level(answers, top)),
            None => calibrated,
        }
    }
}

/// A target Choice a deliberating grounding settles.
#[derive(Debug, Clone)]
pub(super) struct Offer<'a> {
    pub(super) screen: &'a Screen,
    pub(super) purpose: &'a str,
    /// The candidates the Choice offered, in key order.
    pub(super) pool: Vec<Candidate>,
    pub(super) keys: Vec<String>,
    /// The request that asked it, to widen.
    pub(super) request: EvaluationRequest,
    /// The bar its winner must clear.
    pub(super) bar: Bar,
    /// A pick made independently of this Choice — the flat Choice beside a
    /// narrowing tree — that it should agree with.
    pub(super) cross: Option<Candidate>,
}

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// The ballot `id` was answered with in the latest decision that asked
    /// it.
    pub(super) fn ballot(&self, id: &str) -> &[Answer] {
        self.ballots.get(id).map_or(&[], Vec::as_slice)
    }

    /// Each framing's own reading of `belief`, from the latest ballots.
    fn framed(&self, belief: &Belief<'_>) -> Vec<f64> {
        let ids = [Some(belief.yes), Some(belief.no), belief.top]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let framings = ids
            .iter()
            .map(|id| self.ballot(id).len())
            .max()
            .unwrap_or_default();
        (0..framings)
            .filter_map(|index| {
                let answers = ids
                    .iter()
                    .filter_map(|id| Some(((*id).to_owned(), self.ballot(id).get(index)?.clone())))
                    .collect::<BTreeMap<_, _>>();
                belief.read(&answers)
            })
            .collect()
    }

    /// Journals a verdict and marks the evidence loop used.
    fn weighed(&self, log: &mut StepLog, site: &str, evidence: &Evidence, verdict: Verdict) {
        log.used(FlowLoop::Evidence);
        self.runtime.journal.record("evidence", || {
            json!({
                "step": self.step,
                "site": site,
                "p": evidence.p,
                "margin": evidence.margin,
                "agreement": evidence.agreement,
                "spread": evidence.spread,
                "framings": evidence.framings,
                "verdict": verdict.name(),
            })
        });
    }

    fn climbed(&self, log: &mut StepLog, site: &str, rung: &str, verdict: Option<Verdict>) {
        log.used(FlowLoop::Escalation);
        self.runtime.journal.record("escalate", || {
            json!({
                "step": self.step,
                "site": site,
                "rung": rung,
                "verdict": verdict.map(Verdict::name),
            })
        });
    }

}
