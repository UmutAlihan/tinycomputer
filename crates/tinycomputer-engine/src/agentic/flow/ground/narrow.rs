//! Grounding's first steps: grounding memory, the opening round, and
//! narrowing a crowded pool region by region down to one Choice.

use std::collections::BTreeMap;

use serde_json::json;
use tinycomputer_bus::FlowLoop;
use tinyinference_decisions::Answer;

use crate::agentic::flow::denoise;

use crate::agentic::flow::{
    AgentBackend, FlowRun, Halt, StepLog,
    ask::{self, CAP, Questions, chosen, corroborate, elements, numbered, probability},
    memory::recall,
    view::{Candidate, Screen, distinct, label, named_first},
};

use super::{AGREED, BRANCH_MARGIN, First, Grounded, Opening, split, winners};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Picks the element of `pool` that serves `purpose`, or `None` when no
    /// element does with enough agreement.
    ///
    /// `key` identifies the step for grounding memory.
    pub(in crate::agentic::flow) async fn ground(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        purpose: &str,
        key: &str,
        pool: Vec<Candidate>,
    ) -> Result<Option<Grounded>, Halt> {
        let opening = self.opening(log, screen, purpose, key, pool, true);
        self.resume(log, screen, opening, None).await
    }

    /// Grounding's first round for `purpose` over `pool`, without asking
    /// it: grounding memory's confirmation, the narrowing round, or the
    /// Choice. `remember` offers a remembered element first.
    pub(in crate::agentic::flow) fn opening(
        &self,
        log: &mut StepLog,
        screen: &Screen,
        purpose: &str,
        key: &str,
        pool: Vec<Candidate>,
        remember: bool,
    ) -> Opening {
        let mut pool = distinct(pool, self.include_values);
        if self.deliberates(FlowLoop::Denoise) {
            let ranked = denoise::rank(&pool);
            if ranked.len() != pool.len()
                || ranked
                    .iter()
                    .zip(&pool)
                    .any(|(ranked, pooled)| ranked.ref_id != pooled.ref_id)
            {
                log.used(FlowLoop::Denoise);
            }
            pool = ranked;
        }
        if self.wide() {
            named_first(purpose, &mut pool);
        }
        let first = self.first_round(log, screen, purpose, key, &pool, remember);
        Opening {
            purpose: purpose.to_owned(),
            pool,
            first,
        }
    }

    fn first_round(
        &self,
        log: &mut StepLog,
        screen: &Screen,
        purpose: &str,
        key: &str,
        pool: &[Candidate],
        remember: bool,
    ) -> First {
        if pool.is_empty() {
            return First::Settled(None);
        }
        if remember
            && self.enabled(FlowLoop::Memory)
            && let Some(known) = recall(&self.memory, &self.app, key, pool).cloned()
        {
            log.used(FlowLoop::Memory);
            if !self.enabled(FlowLoop::Corroboration) {
                return First::Settled(Some(Grounded {
                    candidate: known,
                    confidence: 1.0,
                }));
            }
            log.used(FlowLoop::Corroboration);
            let request = ask::request(
                self.model(),
                self.state(screen, purpose),
                Questions::default()
                    .with("confirm", corroborate(purpose, &known, self.include_values)),
            );
            return First::Remembered { known, request };
        }
        if pool.len() > CAP && self.enabled(FlowLoop::Narrowing) {
            log.used(FlowLoop::Narrowing);
            return self.narrowing(screen, purpose, pool);
        }
        let pool = &pool[..pool.len().min(crate::agentic::flow::view::MAX_CANDIDATES)];
        let keys = numbered(pool.len());
        let request = ask::request(
            self.model(),
            self.state(screen, purpose),
            Questions::default().with(
                "target",
                elements(purpose, pool, &keys, self.include_values),
            ),
        );
        First::Chosen { keys, request }
    }

    /// The narrowing round: a knockout of [`CAP`]-sized groups, cut along
    /// the screen's regions when they fit in one knockout, and the region
    /// question, asked at once. The region's answer then keeps the winners
    /// it holds — the map a person reads before the detail — without a
    /// round trip of its own.
    fn narrowing(&self, screen: &Screen, purpose: &str, pool: &[Candidate]) -> First {
        let regions = split(pool, 0).map(|(_, regions)| regions);
        let aligned = regions.as_ref().map(|regions| {
            regions
                .iter()
                .enumerate()
                .flat_map(|(index, (_, members))| {
                    members
                        .chunks(CAP)
                        .map(move |chunk| (Some(index), chunk.to_vec()))
                })
                .collect::<Vec<_>>()
        });
        let groups = match aligned {
            Some(groups) if groups.len() <= CAP => groups,
            _ => pool
                .chunks(CAP)
                .take(CAP)
                .map(|chunk| (None, chunk.to_vec()))
                .collect(),
        };
        let mut questions = Questions::default();
        for (index, (_, group)) in groups.iter().enumerate() {
            questions = questions.with(
                &format!("group_{index}"),
                elements(purpose, group, &numbered(group.len()), self.include_values),
            );
        }
        let mut requests = vec![ask::request(
            self.model(),
            self.state(screen, purpose),
            questions,
        )];
        let regions = regions.map(|regions| {
            let keys = numbered(regions.len());
            let question = ask::options(
                json!({
                    "task": "Choose the region of the screen that contains the element for this purpose.",
                    "purpose": purpose,
                }),
                keys.iter()
                    .cloned()
                    .zip(regions.iter().map(|(region, members)| {
                        json!({"untrusted_accessibility_data": {
                            "region": region,
                            "elements": members.len(),
                            "examples": members.iter().take(6).map(label).collect::<Vec<_>>(),
                        }})
                    })),
            );
            requests.push(ask::request(
                self.model(),
                self.state(screen, purpose),
                Questions::default().with("region", question),
            ));
            (keys, regions)
        });
        First::Narrowed {
            groups,
            regions,
            requests,
        }
    }

    /// Finishes grounding from its opening, with the opening's answers when
    /// they were already asked (batched with another request) — or asks them
    /// now.
    pub(in crate::agentic::flow) async fn resume(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        opening: Opening,
        answered: Option<Vec<BTreeMap<String, Answer>>>,
    ) -> Result<Option<Grounded>, Halt> {
        let requests = opening.requests();
        let answers = match answered {
            Some(answers) if answers.len() >= requests.len() => answers,
            _ if requests.is_empty() => Vec::new(),
            _ => self.ask_batch(log, requests).await?,
        };
        let Opening {
            purpose,
            pool,
            first,
        } = opening;
        match first {
            First::Settled(grounded) => Ok(grounded),
            First::Remembered { known, .. } => {
                let confirmed = answers
                    .first()
                    .and_then(|answers| probability(answers, "confirm"))
                    .unwrap_or_default();
                if confirmed >= AGREED {
                    return Ok(Some(Grounded {
                        candidate: known,
                        confidence: confirmed,
                    }));
                }
                let opening = self.opening(log, screen, &purpose, "", pool, false);
                Box::pin(self.resume(log, screen, opening, None)).await
            }
            First::Narrowed {
                groups, regions, ..
            } => {
                let kept = regions.as_ref().map_or_else(Vec::new, |(keys, _)| {
                    answers
                        .get(1)
                        .map_or_else(Vec::new, |answers| self.kept_regions(log, answers, keys))
                });
                let Some(knockout) = answers.first() else {
                    return Ok(None);
                };
                let every = winners(knockout, groups.clone(), &[], regions.as_ref());
                let chosen = winners(knockout, groups, &kept, regions.as_ref());
                let wider = (self.deep()
                    && self.deliberates(FlowLoop::TreeGrounding)
                    && every.len() > chosen.len())
                .then_some(every);
                self.decide(log, screen, &purpose, chosen, wider).await
            }
            First::Chosen { keys, request } => {
                let pool = &pool[..pool.len().min(crate::agentic::flow::view::MAX_CANDIDATES)];
                match answers.first() {
                    Some(answers) => {
                        self.settle(
                            log,
                            screen,
                            &purpose,
                            (pool.to_vec(), &keys),
                            answers,
                            (request, None),
                        )
                        .await
                    }
                    None => Ok(None),
                }
            }
        }
    }

    /// The shared question state for `purpose` on `screen`: under the wide
    /// strategy, the screen as a digest and the run's working memory.
    pub(in crate::agentic::flow) fn state(
        &self,
        screen: &Screen,
        purpose: &str,
    ) -> serde_json::Value {
        if self.wide() {
            return self.wide_state(screen, purpose);
        }
        let mut state = if self.deliberates(FlowLoop::Denoise) {
            let history = denoise::compact(&self.history);
            ask::state(screen, purpose, &history, self.include_values)
        } else {
            ask::state(screen, purpose, &self.history, self.include_values)
        };
        if let Some(collected) = self.collected() {
            state["already_collected"] = collected;
        }
        state
    }

    /// The regions narrowing follows: the chosen one, and the runner-up too
    /// when a deliberating run finds the choice close — the tree's beam.
    fn kept_regions(
        &self,
        log: &mut StepLog,
        answers: &BTreeMap<String, Answer>,
        keys: &[String],
    ) -> Vec<usize> {
        let Some((choice, _)) = chosen(answers, "region") else {
            return Vec::new();
        };
        let Some(first) = keys.iter().position(|key| *key == choice) else {
            return Vec::new();
        };
        let mut kept = vec![first];
        if !self.deliberates(FlowLoop::TreeGrounding) {
            return kept;
        }
        let Some(Answer::Choice(region)) = answers.get("region") else {
            return kept;
        };
        let lead = region
            .probabilities
            .get(&choice)
            .copied()
            .unwrap_or_default();
        let second = keys
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != first)
            .filter_map(|(index, key)| Some((index, region.probabilities.get(key).copied()?)))
            .max_by(|left, right| left.1.total_cmp(&right.1));
        if let Some((index, probability)) = second
            && lead - probability < BRANCH_MARGIN
        {
            log.used(FlowLoop::TreeGrounding);
            kept.push(index);
        }
        kept
    }
}
