//! Picking one element for a purpose, with as few and as small questions as
//! the screen allows.
//!
//! 1. **Memory**: an element that grounded the same step before is offered
//!    first and confirmed with one yes/no question.
//! 2. **Narrowing**: a pool larger than [`CAP`] is split by region (the
//!    ancestor it sits under) and Jev picks a region before an element. If
//!    regions do not split it, a knockout of `CAP`-sized groups does.
//! 3. **Choice** over at most `CAP` elements.
//! 4. **Consistency and corroboration**: a low-confidence pick is re-asked
//!    with relabelled options, and confirmed with a yes/no question, in one
//!    request. It is used only if the evidence agrees.

use std::collections::BTreeMap;

use serde_json::json;
use tinycomputer_bus::FlowLoop;

use super::{
    AgentBackend, FlowRun, Halt, StepLog,
    ask::{self, CAP, Questions, chosen, corroborate, elements, lettered, numbered, probability},
    memory::recall,
    view::{ACT, Candidate, Screen, exact_named_match, label, named_first},
};

/// Least probability an exact-name match needs to be used without re-asking.
pub(super) const NAMED_FLOOR: f64 = 0.45;
/// A corroboration this confident accepts a target on its own.
pub(super) const CORROBORATED: f64 = 0.8;
/// A corroboration this confident accepts a target the re-ask agreed on.
pub(super) const AGREED: f64 = 0.5;
/// Deepest ancestor level narrowing splits on.
const MAX_REGION_DEPTH: usize = 8;
/// Region rounds before the knockout takes over.
const MAX_REGION_ROUNDS: usize = 3;

/// Named groups of candidates, largest first.
pub(super) type Regions = Vec<(String, Vec<Candidate>)>;

/// An element chosen for a purpose.
#[derive(Debug, Clone)]
pub(super) struct Grounded {
    pub(super) candidate: Candidate,
    pub(super) confidence: f64,
}

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Picks the element of `pool` that serves `purpose`, or `None` when no
    /// element does with enough agreement.
    ///
    /// `key` identifies the step for grounding memory.
    pub(super) async fn ground(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        purpose: &str,
        key: &str,
        mut pool: Vec<Candidate>,
    ) -> Result<Option<Grounded>, Halt> {
        if pool.is_empty() {
            return Ok(None);
        }
        if self.wide() {
            named_first(purpose, &mut pool);
        }
        if self.enabled(FlowLoop::Memory)
            && let Some(known) = recall(&self.memory, &self.app, key, &pool).cloned()
        {
            log.used(FlowLoop::Memory);
            let confirmed = if self.enabled(FlowLoop::Corroboration) {
                log.used(FlowLoop::Corroboration);
                let answers = self
                    .ask(
                        log,
                        ask::request(
                            self.model(),
                            self.state(screen, purpose),
                            Questions::default()
                                .with("confirm", corroborate(purpose, &known, self.include_values)),
                        ),
                    )
                    .await?;
                probability(&answers, "confirm").unwrap_or_default()
            } else {
                1.0
            };
            if confirmed >= AGREED {
                return Ok(Some(Grounded {
                    candidate: known,
                    confidence: confirmed,
                }));
            }
        }
        let pool = if pool.len() > CAP && self.enabled(FlowLoop::Narrowing) {
            log.used(FlowLoop::Narrowing);
            self.narrow(log, screen, purpose, pool).await?
        } else {
            pool
        };
        self.decide(log, screen, purpose, pool).await
    }

    /// The shared question state for `purpose` on `screen`: under the wide
    /// strategy, the screen as a digest and the run's working memory.
    pub(super) fn state(&self, screen: &Screen, purpose: &str) -> serde_json::Value {
        if self.wide() {
            return self.wide_state(screen, purpose);
        }
        ask::state(screen, purpose, &self.history, self.include_values)
    }

    /// Shrinks `pool` to at most [`CAP`] elements.
    async fn narrow(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        purpose: &str,
        mut pool: Vec<Candidate>,
    ) -> Result<Vec<Candidate>, Halt> {
        let mut depth = 0;
        for _ in 0..MAX_REGION_ROUNDS {
            if pool.len() <= CAP {
                return Ok(pool);
            }
            let Some((level, regions)) = split(&pool, depth) else {
                break;
            };
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
            let answers = self
                .ask(
                    log,
                    ask::request(
                        self.model(),
                        self.state(screen, purpose),
                        Questions::default().with("region", question),
                    ),
                )
                .await?;
            let Some((choice, _)) = chosen(&answers, "region") else {
                break;
            };
            let Some(index) = keys.iter().position(|key| *key == choice) else {
                break;
            };
            pool = regions
                .into_iter()
                .nth(index)
                .map(|(_, members)| members)
                .unwrap_or_default();
            depth = level + 1;
        }
        if pool.len() <= CAP {
            return Ok(pool);
        }
        self.knockout(log, screen, purpose, pool).await
    }

    /// Picks a winner from each `CAP`-sized group in one request.
    async fn knockout(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        purpose: &str,
        pool: Vec<Candidate>,
    ) -> Result<Vec<Candidate>, Halt> {
        let groups = pool
            .chunks(CAP)
            .take(CAP)
            .map(<[Candidate]>::to_vec)
            .collect::<Vec<_>>();
        let mut questions = Questions::default();
        for (index, group) in groups.iter().enumerate() {
            questions = questions.with(
                &format!("group_{index}"),
                elements(purpose, group, &numbered(group.len()), self.include_values),
            );
        }
        let answers = self
            .ask(
                log,
                ask::request(self.model(), self.state(screen, purpose), questions),
            )
            .await?;
        Ok(groups
            .into_iter()
            .enumerate()
            .filter_map(|(index, group)| {
                let (choice, _) = chosen(&answers, &format!("group_{index}"))?;
                let position = choice.parse::<usize>().ok()?.checked_sub(1)?;
                group.into_iter().nth(position)
            })
            .collect())
    }

    /// The final Choice, re-asked and corroborated when it is not confident.
    pub(super) async fn decide(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        purpose: &str,
        mut pool: Vec<Candidate>,
    ) -> Result<Option<Grounded>, Halt> {
        pool.truncate(super::view::MAX_CANDIDATES);
        if pool.is_empty() {
            return Ok(None);
        }
        let keys = numbered(pool.len());
        let answers = self
            .ask(
                log,
                ask::request(
                    self.model(),
                    self.state(screen, purpose),
                    Questions::default().with(
                        "target",
                        elements(purpose, &pool, &keys, self.include_values),
                    ),
                ),
            )
            .await?;
        let Some((choice, confidence)) = chosen(&answers, "target") else {
            return Ok(None);
        };
        let Some(first) = keys
            .iter()
            .position(|key| *key == choice)
            .and_then(|index| pool.get(index).cloned())
        else {
            return Ok(None);
        };
        if confidence >= ACT
            || (confidence >= NAMED_FLOOR && exact_named_match(purpose, Some(&first)))
        {
            return Ok(Some(Grounded {
                candidate: first,
                confidence,
            }));
        }
        let consistency = self.enabled(FlowLoop::Consistency) && pool.len() > 1;
        let corroboration = self.enabled(FlowLoop::Corroboration);
        if !consistency && !corroboration {
            return Ok(None);
        }
        let mut questions = Questions::default();
        let mut reordered = pool.clone();
        reordered.reverse();
        let letters = lettered(reordered.len());
        if consistency {
            log.used(FlowLoop::Consistency);
            questions = questions.with(
                "again",
                elements(purpose, &reordered, &letters, self.include_values),
            );
        }
        if corroboration {
            log.used(FlowLoop::Corroboration);
            questions =
                questions.with("confirm", corroborate(purpose, &first, self.include_values));
        }
        let answers = self
            .ask(
                log,
                ask::request(self.model(), self.state(screen, purpose), questions),
            )
            .await?;
        let again = chosen(&answers, "again").and_then(|(choice, probability)| {
            letters
                .iter()
                .position(|key| *key == choice)
                .and_then(|index| reordered.get(index))
                .map(|candidate| (candidate.clone(), probability))
        });
        let confirm = probability(&answers, "confirm");
        let agrees = again
            .as_ref()
            .is_some_and(|(candidate, _)| candidate.ref_id == first.ref_id);
        let accepted = match (consistency, corroboration) {
            (true, true) => {
                (agrees && confirm.unwrap_or_default() >= AGREED)
                    || confirm.unwrap_or_default() >= CORROBORATED
            }
            (true, false) => agrees,
            (false, _) => confirm.unwrap_or_default() >= CORROBORATED,
        };
        if !accepted {
            return Ok(None);
        }
        let again_confidence = again.map_or(0.0, |(_, probability)| probability);
        Ok(Some(Grounded {
            candidate: first,
            confidence: confidence
                .max(again_confidence)
                .max(confirm.unwrap_or_default()),
        }))
    }
}

/// Groups `pool` by the first ancestor level, at or below `from`, that splits
/// it into more than one region. Regions beyond `CAP - 1` are merged.
pub(super) fn split(pool: &[Candidate], from: usize) -> Option<(usize, Regions)> {
    for level in from..MAX_REGION_DEPTH {
        let mut regions: BTreeMap<String, Vec<Candidate>> = BTreeMap::new();
        for candidate in pool {
            let region = candidate
                .path
                .get(level)
                .cloned()
                .unwrap_or_else(|| "top level".to_owned());
            regions.entry(region).or_default().push(candidate.clone());
        }
        if regions.len() < 2 {
            continue;
        }
        let mut regions = regions.into_iter().collect::<Vec<_>>();
        regions.sort_by_key(|(_, members)| std::cmp::Reverse(members.len()));
        if regions.len() > CAP {
            let rest = regions
                .split_off(CAP - 1)
                .into_iter()
                .flat_map(|(_, members)| members)
                .collect::<Vec<_>>();
            regions.push(("everything else".to_owned(), rest));
        }
        return Some((level, regions));
    }
    None
}
