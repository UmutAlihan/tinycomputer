//! The ladder for a target Choice: more framings, a duel between the
//! finalists, and at the deep level a contrast; and vouching for a single
//! candidate before an irreversible press.

use std::collections::BTreeMap;

use serde_json::json;
use tinycomputer_bus::FlowLoop;
use tinyinference_decisions::Answer;

use crate::agentic::flow::{
    AgentBackend, FlowRun, Halt, StepLog,
    ask::{self, Questions},
    duel,
    evidence::{self, Verdict},
    ground::Grounded,
    view::{Candidate, Screen},
};

use super::{CONTRAST_ACCEPT, CONTRAST_LEAD, MAX_FRONTIER, Offer};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Settles a target Choice on its evidence: acted on when its winner is
    /// clearly ahead and agreed on, abstained from when nothing serves, and
    /// otherwise taken up the ladder — more framings, a duel between the
    /// finalists, and at the deep level a contrast. `answers` holds the
    /// Choice as `target`.
    pub(in crate::agentic::flow) async fn deliberate_target(
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
        // Deliberation changes a pick; it refuses one only when nothing
        // serves (`Verdict::Abstain`, above). A close call no rung settles
        // is acted on at its best ranking, with the runners-up kept for a
        // backtrack: in a `do` loop, pressing nothing stalls the step, and
        // the effect check and undo are there to catch a wrong press.
        let Some((ranked, champion, shares)) = self.duel(log, &offer, finalists).await? else {
            return Ok(pick.map(|(candidate, probability)| {
                self.frontier = runners_up(&offer, merged, &candidate);
                Grounded {
                    candidate,
                    confidence: probability,
                }
            }));
        };
        let contrasted = if champion.is_none() && self.deep() && self.enabled(FlowLoop::Escalation)
        {
            self.contrast(log, &offer, &ranked).await?
        } else {
            None
        };
        let (candidate, belief) = contrasted.unwrap_or_else(|| {
            let best = champion.unwrap_or(0);
            // The finalist's own mean share of its pairings — a measured
            // belief, whether or not it swept every rival — never a
            // constant that would clear any later bar (`IRREVERSIBLE_FLOOR`
            // included) regardless of how close the duel actually was.
            (ranked[best].clone(), shares[best])
        });
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

    /// The finalists ranked by a duel, the champion's index among them, and
    /// each ranked finalist's own mean share of its pairings — a measured
    /// belief, `1.0` only for the one finalist a skipped duel never had to
    /// compare. `None` when there are no finalists, or the duel loop is off.
    async fn duel(
        &mut self,
        log: &mut StepLog,
        offer: &Offer<'_>,
        finalists: Vec<Candidate>,
    ) -> Result<Option<(Vec<Candidate>, Option<usize>, Vec<f64>)>, Halt> {
        match finalists.len() {
            0 => return Ok(None),
            1 => return Ok(Some((finalists, Some(0), vec![1.0]))),
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
                "finalists": finalists.iter().map(crate::agentic::flow::view::label).collect::<Vec<_>>(),
                "wins": standing.wins,
                "champion": standing.champion,
            })
        });
        let champion = standing
            .champion
            .and_then(|champion| standing.order.iter().position(|index| *index == champion));
        // Each finalist's summed share (`Standing::strength`) is over one
        // pairing with every other finalist — `finalists.len() - 1` of
        // them, at least one here since the single-finalist case already
        // returned above — so dividing by that count reads it back as a
        // mean share, in `[0, 1]` like any other belief.
        let pairings = f64::from(u32::try_from(finalists.len() - 1).unwrap_or(u32::MAX));
        let ranked = standing
            .order
            .iter()
            .map(|index| finalists[*index].clone())
            .collect();
        let shares = standing
            .order
            .iter()
            .map(|index| standing.strength[*index] / pairings)
            .collect();
        Ok(Some((ranked, champion, shares)))
    }

    /// The better of the two leading finalists of a duel with no champion,
    /// with its belief, when a contrast shows it clearly better; `None`
    /// leaves the duel's ranking to decide.
    async fn contrast(
        &mut self,
        log: &mut StepLog,
        offer: &Offer<'_>,
        ranked: &[Candidate],
    ) -> Result<Option<(Candidate, f64)>, Halt> {
        let tested = ranked.iter().take(2).cloned().collect::<Vec<_>>();
        if self.room() == 0 || tested.len() < 2 {
            return Ok(None);
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
        let best = usize::from(beliefs[1] > beliefs[0]);
        let other = beliefs[1 - best];
        let settled = (beliefs[best] >= CONTRAST_ACCEPT && beliefs[best] - other >= CONTRAST_LEAD)
            .then(|| (tested[best].clone(), beliefs[best]));
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

    /// How strongly `candidate` is the element for `purpose`: "is it?"
    /// calibrated against "is it only similar or next to it?", asked in
    /// every framing the run can afford.
    pub(in crate::agentic::flow) async fn vouch(
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
