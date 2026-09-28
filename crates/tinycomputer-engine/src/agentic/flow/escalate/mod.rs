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
//! 3. **Contrast** (deep only): each remaining finalist asked "is this the
//!    element?" beside "is this only something similar or next to it?".
//! 4. **Views** (deep only): a yes/no judgement asked again over other
//!    renderings of the screen — the screen alone, without the history that
//!    can lead it, and what changed since the step began. Views that fall on
//!    both sides of the threshold keep the judgement open.
//!
//! Every rung is one `FlowRun::ask`, so the budget, masking, voting, and
//! journal all apply, and every rung first checks the budget has room: a
//! run short of calls stops climbing and decides with what it has, rather
//! than failing for lack of deliberation.

use std::collections::BTreeMap;

use serde_json::json;
use tinycomputer_bus::FlowLoop;
use tinyinference_decisions::{Answer, EvaluationRequest};

use super::{
    AgentBackend, FlowRun, Halt, StepLog,
    ask::{self, Questions},
    duel,
    evidence::{self, Bar, Evidence, Verdict},
    ground::Grounded,
    view::{Candidate, Screen},
    vote,
};

/// Least calibrated belief that the duel's champion is the element, when a
/// contrast is asked about it.
pub(super) const CONTRAST_KEEP: f64 = 0.5;
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

    /// Asks `request` again in the framings it was not asked in yet, up to
    /// [`vote::MAX_VOTES`] and within the budget, and returns every one of
    /// its questions re-tallied over the whole ballot. `None` when there is
    /// no framing left to ask or no budget to ask it with.
    pub(super) async fn widen(
        &mut self,
        log: &mut StepLog,
        request: &EvaluationRequest,
    ) -> Result<Option<BTreeMap<String, Answer>>, Halt> {
        let asked = request
            .questions
            .keys()
            .map(|id| self.ballot(id).len())
            .max()
            .unwrap_or_default();
        let from = u32::try_from(asked).unwrap_or(u32::MAX);
        let to = vote::MAX_VOTES.min(from.saturating_add(self.room()));
        if from >= to || !self.enabled(FlowLoop::Vote) {
            return Ok(None);
        }
        let prepared = self.outgoing(log, request.clone());
        let framings = vote::framings_between(&prepared, from, to);
        let handles = self.spawn(&framings);
        self.rounds = self.rounds.saturating_add(1);
        self.decisions = self.decisions.saturating_add(1);
        let mut answered = Vec::new();
        for (framing, handle) in framings.into_iter().zip(handles) {
            if let Ok(Ok(evaluation)) = handle.await {
                super::merge_metrics(&mut self.metrics, &evaluation);
                log.calls = log.calls.saturating_add(1);
                answered.push((framing, evaluation.response.answers));
            }
        }
        if answered.is_empty() {
            return Ok(None);
        }
        for (id, fresh) in vote::ballots(&answered) {
            self.ballots.entry(id).or_default().extend(fresh);
        }
        let ballots = request
            .questions
            .keys()
            .filter_map(|id| Some((id.clone(), self.ballots.get(id)?.clone())))
            .collect::<BTreeMap<_, _>>();
        Ok(Some(vote::tally(&ballots)))
    }

    /// Settles a yes/no judgement `answers` hold for `request`: taken as it
    /// reads when its framings agree clearly, otherwise widened and, at the
    /// deep level, asked again over `views` — requests over other
    /// renderings of the screen asking `belief.yes` and `belief.no`.
    ///
    /// Returns the settled belief; `answers` is updated with whatever the
    /// widening re-tallied, so the rest of the request (a move, a progress
    /// level) is read from the larger ballot too.
    pub(super) async fn settle_belief(
        &mut self,
        log: &mut StepLog,
        belief: Belief<'_>,
        request: &EvaluationRequest,
        answers: &mut BTreeMap<String, Answer>,
        views: Vec<EvaluationRequest>,
    ) -> Result<Option<f64>, Halt> {
        let Some(mut held) = belief.read(answers) else {
            return Ok(None);
        };
        if !self.deliberates(FlowLoop::Evidence) {
            return Ok(Some(held));
        }
        let mut framed = self.framed(&belief);
        if framed.is_empty() {
            framed.push(held);
        }
        let weighed = evidence::of_beliefs(&framed, belief.threshold);
        let verdict = evidence::belief_verdict(&weighed, belief.threshold);
        self.weighed(log, belief.site, &weighed, verdict);
        if verdict == Verdict::Accept || !self.enabled(FlowLoop::Escalation) {
            return Ok(Some(held));
        }
        if let Some(widened) = self.widen(log, request).await? {
            answers.extend(widened);
            held = belief.read(answers).unwrap_or(held);
            let weighed = evidence::of_beliefs(&self.framed(&belief), belief.threshold);
            let verdict = evidence::belief_verdict(&weighed, belief.threshold);
            self.climbed(log, belief.site, "framings", Some(verdict));
            self.weighed(log, belief.site, &weighed, verdict);
            if verdict == Verdict::Accept {
                return Ok(Some(held));
            }
        }
        if !self.deep() || views.is_empty() || self.room() == 0 {
            return Ok(Some(held));
        }
        let seen = self.ask_batch(log, views).await?;
        let mut readings = vec![held];
        readings.extend(
            seen.iter()
                .filter_map(|answers| ask::calibrated(answers, belief.yes, belief.no)),
        );
        let above = readings
            .iter()
            .filter(|reading| **reading >= belief.threshold);
        let settled = if above.count() % readings.len() == 0 {
            evidence::mean(&readings)
        } else {
            readings.iter().copied().fold(f64::INFINITY, f64::min)
        };
        self.climbed(
            log,
            belief.site,
            "views",
            Some(if settled >= belief.threshold {
                Verdict::Accept
            } else {
                Verdict::Deliberate
            }),
        );
        self.runtime.journal.record("views", || {
            json!({
                "step": self.step,
                "site": belief.site,
                "readings": readings,
                "settled": settled,
            })
        });
        Ok(Some(settled))
    }

    /// Settles a target Choice on its evidence: acted on when its winner is
    /// clearly ahead and agreed on, abstained from when nothing serves, and
    /// otherwise taken up the ladder — more framings, a duel between the
    /// finalists, and at the deep level a contrast. `answers` holds the
    /// Choice as `target`.
    pub(super) async fn deliberate_target(
        &mut self,
        log: &mut StepLog,
        offer: Offer<'_>,
        mut answers: BTreeMap<String, Answer>,
    ) -> Result<Option<Grounded>, Halt> {
        let mut verdict = self.target_verdict(log, &offer, &answers);
        if verdict == Verdict::Deliberate
            && self.enabled(FlowLoop::Escalation)
            && let Some(widened) = self.widen(log, &offer.request).await?
        {
            answers.extend(widened);
            verdict = self.target_verdict(log, &offer, &answers);
            self.climbed(log, "target", "framings", Some(verdict));
        }
        let Some(merged) = answers.get("target") else {
            return Ok(None);
        };
        let pick = picked(&offer, merged);
        match verdict {
            Verdict::Abstain => return Ok(None),
            Verdict::Accept => {
                let Some((candidate, probability)) = pick else {
                    return Ok(None);
                };
                self.frontier = runners_up(&offer, merged, &candidate);
                return Ok(Some(Grounded {
                    candidate,
                    confidence: probability,
                }));
            }
            Verdict::Deliberate => {}
        }
        let mut finalists = duel::finalists(merged)
            .iter()
            .filter_map(|key| {
                let index = offer.keys.iter().position(|offered| offered == key)?;
                offer.pool.get(index).cloned()
            })
            .collect::<Vec<_>>();
        if let Some(cross) = &offer.cross
            && !finalists
                .iter()
                .any(|finalist| finalist.ref_id == cross.ref_id)
        {
            finalists.truncate(duel::MAX_FINALISTS - 1);
            finalists.push(cross.clone());
        }
        let ranked = self.duel(log, &offer, finalists).await?;
        let Some((ranked, champion)) = ranked else {
            return Ok(None);
        };
        let chosen = if self.deep() && self.enabled(FlowLoop::Escalation) {
            self.contrast(log, &offer, &ranked, champion).await?
        } else {
            champion.map(|champion| (ranked[champion].clone(), 1.0))
        };
        let Some((candidate, belief)) = chosen else {
            return Ok(None);
        };
        let probability = pick
            .as_ref()
            .filter(|(first, _)| first.ref_id == candidate.ref_id)
            .map_or(0.0, |(_, probability)| *probability);
        let mut frontier = ranked
            .into_iter()
            .filter(|finalist| finalist.ref_id != candidate.ref_id)
            .collect::<Vec<_>>();
        frontier.extend(runners_up(&offer, merged, &candidate));
        frontier.dedup_by(|left, right| left.ref_id == right.ref_id);
        frontier.truncate(MAX_FRONTIER);
        self.frontier = frontier;
        Ok(Some(Grounded {
            candidate,
            confidence: probability.max(belief),
        }))
    }

    /// The verdict on `answers`' target Choice against `offer`'s bar, with
    /// its cross-check: a pick the independent Choice agrees with is taken
    /// unless nothing serves; one it disagrees with is never taken as read.
    fn target_verdict(
        &self,
        log: &mut StepLog,
        offer: &Offer<'_>,
        answers: &BTreeMap<String, Answer>,
    ) -> Verdict {
        let Some(merged) = answers.get("target") else {
            return Verdict::Abstain;
        };
        let Some(weighed) = evidence::of_choice(merged, self.ballot("target")) else {
            return Verdict::Abstain;
        };
        let mut verdict = evidence::choice_verdict(&weighed, &offer.bar);
        if let (Some(cross), Some((pick, _))) = (&offer.cross, picked(offer, merged)) {
            log.used(FlowLoop::TreeGrounding);
            verdict = match (verdict, cross.ref_id == pick.ref_id) {
                (Verdict::Abstain, _) => Verdict::Abstain,
                (_, true) => Verdict::Accept,
                (_, false) => Verdict::Deliberate,
            };
        }
        self.weighed(log, "target", &weighed, verdict);
        verdict
    }

    /// The finalists ranked by a duel, and the champion's index among them.
    /// One finalist needs no duel; `None` when there is none, or the duel
    /// loop is off.
    async fn duel(
        &mut self,
        log: &mut StepLog,
        offer: &Offer<'_>,
        finalists: Vec<Candidate>,
    ) -> Result<Option<(Vec<Candidate>, Option<usize>)>, Halt> {
        match finalists.len() {
            0 => return Ok(None),
            1 => return Ok(Some((finalists, Some(0)))),
            _ => {}
        }
        if !self.deliberates(FlowLoop::Duel) || self.room() == 0 {
            return Ok(None);
        }
        log.used(FlowLoop::Duel);
        let mut questions = Questions::default();
        for (id, question) in duel::questions(offer.purpose, &finalists, self.include_values) {
            questions = questions.with(&id, question);
        }
        let answers = self
            .ask(
                log,
                ask::request(
                    self.model(),
                    self.state(offer.screen, offer.purpose),
                    questions,
                ),
            )
            .await?;
        let standing = duel::standing(&answers, finalists.len());
        self.runtime.journal.record("duel", || {
            json!({
                "step": self.step,
                "finalists": finalists.iter().map(super::view::label).collect::<Vec<_>>(),
                "wins": standing.wins,
                "champion": standing.champion,
            })
        });
        let champion = standing
            .champion
            .and_then(|champion| standing.order.iter().position(|index| *index == champion));
        let ranked = standing
            .order
            .iter()
            .map(|index| finalists[*index].clone())
            .collect();
        Ok(Some((ranked, champion)))
    }

    /// The finalist a contrast settles on, with its belief: the champion
    /// when it holds up, otherwise the better of the two leading finalists
    /// when it is clearly better.
    async fn contrast(
        &mut self,
        log: &mut StepLog,
        offer: &Offer<'_>,
        ranked: &[Candidate],
        champion: Option<usize>,
    ) -> Result<Option<(Candidate, f64)>, Halt> {
        let tested = match champion {
            Some(champion) => vec![ranked[champion].clone()],
            None => ranked.iter().take(2).cloned().collect(),
        };
        if self.room() == 0 {
            return Ok(champion.map(|champion| (ranked[champion].clone(), 0.0)));
        }
        let mut questions = Questions::default();
        for (index, candidate) in tested.iter().enumerate() {
            questions = questions
                .with(
                    &format!("is_{index}"),
                    ask::corroborate(offer.purpose, candidate, self.include_values),
                )
                .with(
                    &format!("only_near_{index}"),
                    ask::only_near(offer.purpose, candidate, self.include_values),
                );
        }
        let answers = self
            .ask(
                log,
                ask::request(
                    self.model(),
                    self.state(offer.screen, offer.purpose),
                    questions,
                ),
            )
            .await?;
        let beliefs = (0..tested.len())
            .map(|index| {
                ask::calibrated(
                    &answers,
                    &format!("is_{index}"),
                    &format!("only_near_{index}"),
                )
                .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        let settled = if champion.is_some() {
            (beliefs[0] >= CONTRAST_KEEP).then(|| (tested[0].clone(), beliefs[0]))
        } else {
            let best = usize::from(beliefs.get(1).is_some_and(|other| *other > beliefs[0]));
            let other = beliefs.get(1 - best).copied().unwrap_or_default();
            (beliefs[best] >= CONTRAST_ACCEPT && beliefs[best] - other >= CONTRAST_LEAD)
                .then(|| (tested[best].clone(), beliefs[best]))
        };
        self.climbed(
            log,
            "target",
            "contrast",
            Some(if settled.is_some() {
                Verdict::Accept
            } else {
                Verdict::Abstain
            }),
        );
        Ok(settled)
    }
}

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// How strongly `candidate` is the element for `purpose`: "is it?"
    /// calibrated against "is it only similar or next to it?", asked in
    /// every framing the run can afford.
    pub(super) async fn vouch(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        purpose: &str,
        candidate: &Candidate,
    ) -> Result<f64, Halt> {
        let request = ask::request(
            self.model(),
            self.state(screen, purpose),
            Questions::default()
                .with(
                    "is_0",
                    ask::corroborate(purpose, candidate, self.include_values),
                )
                .with(
                    "only_near_0",
                    ask::only_near(purpose, candidate, self.include_values),
                ),
        );
        let mut answers = self.ask(log, request.clone()).await?;
        if self.enabled(FlowLoop::Escalation)
            && let Some(widened) = self.widen(log, &request).await?
        {
            self.climbed(log, "irreversible", "framings", None);
            answers.extend(widened);
        }
        Ok(ask::calibrated(&answers, "is_0", "only_near_0").unwrap_or_default())
    }
}

/// The candidate `merged` picked among `offer`'s, with its probability.
fn picked(offer: &Offer<'_>, merged: &Answer) -> Option<(Candidate, f64)> {
    let Answer::Choice(choice) = merged else {
        return None;
    };
    let index = offer.keys.iter().position(|key| *key == choice.choice)?;
    let candidate = offer.pool.get(index)?.clone();
    let probability = choice
        .probabilities
        .get(&choice.choice)
        .copied()
        .unwrap_or_default();
    Some((candidate, probability))
}

/// `offer`'s candidates other than `taken`, by `merged`'s probability, best
/// first: every one Jev gave at least a finalist's share.
fn runners_up(offer: &Offer<'_>, merged: &Answer, taken: &Candidate) -> Vec<Candidate> {
    let Answer::Choice(choice) = merged else {
        return Vec::new();
    };
    let mut ranked = offer
        .keys
        .iter()
        .zip(&offer.pool)
        .filter(|(_, candidate)| candidate.ref_id != taken.ref_id)
        .filter_map(|(key, candidate)| {
            let probability = choice.probabilities.get(key).copied()?;
            (probability >= duel::FINALIST_FLOOR).then_some((probability, candidate))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.0.total_cmp(&left.0));
    ranked
        .into_iter()
        .take(MAX_FRONTIER)
        .map(|(_, candidate)| candidate.clone())
        .collect()
}
