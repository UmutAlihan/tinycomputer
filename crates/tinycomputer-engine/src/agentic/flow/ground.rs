//! Picking one element for a purpose, with as few and as small questions as
//! the screen allows.
//!
//! 1. **Memory**: an element that grounded the same step before is offered
//!    first and confirmed with one yes/no question.
//! 2. **Narrowing**: a pool larger than [`CAP`] is split by region (the
//!    ancestor it sits under). One round trip asks which region holds the
//!    element and a knockout of `CAP`-sized groups cut along the regions;
//!    the chosen region's winners go on to the Choice.
//! 3. **Choice** over at most `CAP` elements.
//! 4. **Consistency and corroboration**: a low-confidence pick is re-asked
//!    with relabelled options, and confirmed with a yes/no question, in one
//!    request. It is used only if the evidence agrees.
//!
//! A deliberating run (`docs/technical/specs/jev-deliberation.md`) changes three
//! things. The pool is denoised first (`denoise.rs`): what is in view ranks
//! ahead of what is not. Narrowing keeps the two best regions wherever the
//! region answer is close — an early wrong branch is the one grounding can
//! never recover from — and at the deep level, when the region answer left
//! group winners out, the final Choice is asked a second time over every
//! winner, to cross-check the region against a pick that never used it. And
//! step 4 gives way to the evidence gate and its escalation ladder
//! (`escalate`).
//!
//! The first round is built by [`FlowRun::opening`] without being asked, so
//! a `do` turn can send it with its judge, and finished by
//! [`FlowRun::resume`].

use std::collections::BTreeMap;

use serde_json::json;
use tinycomputer_bus::FlowLoop;
use tinyinference_decisions::{Answer, EvaluationRequest};

use super::{denoise, escalate::Offer, evidence::Bar};

use super::{
    AgentBackend, FlowRun, Halt, StepLog,
    ask::{self, CAP, Questions, chosen, corroborate, elements, lettered, numbered, probability},
    memory::recall,
    view::{ACT, Candidate, Screen, distinct, exact_named_match, label, named_first},
};

/// Least probability an exact-name match needs to be used without re-asking.
pub(super) const NAMED_FLOOR: f64 = 0.45;
/// A corroboration this confident accepts a target on its own.
pub(super) const CORROBORATED: f64 = 0.8;
/// A corroboration this confident accepts a target the re-ask agreed on.
pub(super) const AGREED: f64 = 0.5;
/// Deepest ancestor level narrowing splits on.
const MAX_REGION_DEPTH: usize = 8;
/// Lead the chosen region needs over the next one for narrowing to follow
/// it alone; closer, a deliberating run keeps both.
pub(super) const BRANCH_MARGIN: f64 = 0.3;

/// Named groups of candidates, largest first.
pub(super) type Regions = Vec<(String, Vec<Candidate>)>;

/// An element chosen for a purpose.
#[derive(Debug, Clone)]
pub(super) struct Grounded {
    pub(super) candidate: Candidate,
    pub(super) confidence: f64,
}

/// The first round grounding asks, built but not yet sent, so a caller can
/// batch it with a request of its own: the turn's judge asks it alongside,
/// and uses the answers only when the move turns out to need a target.
#[derive(Debug, Clone)]
pub(super) struct Opening {
    purpose: String,
    pool: Vec<Candidate>,
    first: First,
}

/// What an [`Opening`] asks.
#[derive(Debug, Clone)]
enum First {
    /// Nothing to ask: no pool, or a remembered element used unconfirmed.
    Settled(Option<Grounded>),
    /// A remembered element, confirmed with one yes/no question.
    Remembered {
        known: Candidate,
        request: EvaluationRequest,
    },
    /// A crowded pool: a knockout whose groups follow the screen's regions,
    /// and, when the pool splits, which region holds the element — asked
    /// together, in one round trip.
    Narrowed {
        groups: Vec<(Option<usize>, Vec<Candidate>)>,
        regions: Option<(Vec<String>, Regions)>,
        requests: Vec<EvaluationRequest>,
    },
    /// A pool small enough for one Choice.
    Chosen {
        keys: Vec<String>,
        request: EvaluationRequest,
    },
}

impl Opening {
    /// The requests to send, the one grounding needs most first.
    pub(super) fn requests(&self) -> Vec<EvaluationRequest> {
        match &self.first {
            First::Settled(_) => Vec::new(),
            First::Remembered { request, .. } | First::Chosen { request, .. } => {
                vec![request.clone()]
            }
            First::Narrowed { requests, .. } => requests.clone(),
        }
    }
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
        pool: Vec<Candidate>,
    ) -> Result<Option<Grounded>, Halt> {
        let opening = self.opening(log, screen, purpose, key, pool, true);
        self.resume(log, screen, opening, None).await
    }

    /// Grounding's first round for `purpose` over `pool`, without asking
    /// it: grounding memory's confirmation, the narrowing round, or the
    /// Choice. `remember` offers a remembered element first.
    pub(super) fn opening(
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
        let pool = &pool[..pool.len().min(super::view::MAX_CANDIDATES)];
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
    pub(super) async fn resume(
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
                let pool = &pool[..pool.len().min(super::view::MAX_CANDIDATES)];
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
    pub(super) fn state(&self, screen: &Screen, purpose: &str) -> serde_json::Value {
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

    /// The final Choice, re-asked and corroborated when it is not confident.
    ///
    /// `wider` is a larger pool the narrowing tree cut `pool` from: a deep
    /// run asks the same Choice over it in the same round trip, and checks
    /// the pick against the one made without the cut.
    pub(super) async fn decide(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        purpose: &str,
        mut pool: Vec<Candidate>,
        wider: Option<Vec<Candidate>>,
    ) -> Result<Option<Grounded>, Halt> {
        pool.truncate(super::view::MAX_CANDIDATES);
        if pool.is_empty() {
            return Ok(None);
        }
        let keys = numbered(pool.len());
        let request = ask::request(
            self.model(),
            self.state(screen, purpose),
            Questions::default().with(
                "target",
                elements(purpose, &pool, &keys, self.include_values),
            ),
        );
        let mut requests = vec![request.clone()];
        let wider = wider.map(|mut wider| {
            wider.truncate(CAP);
            let keys = numbered(wider.len());
            requests.push(ask::request(
                self.model(),
                self.state(screen, purpose),
                Questions::default().with(
                    "wider",
                    elements(purpose, &wider, &keys, self.include_values),
                ),
            ));
            (keys, wider)
        });
        let mut replies = self.ask_batch(log, requests).await?.into_iter();
        let answers = replies.next().unwrap_or_default();
        let cross = wider
            .zip(replies.next())
            .and_then(|((keys, wider), answers)| {
                let (choice, _) = chosen(&answers, "wider")?;
                let index = keys.iter().position(|key| *key == choice)?;
                wider.get(index).cloned()
            });
        self.settle(
            log,
            screen,
            purpose,
            (pool, &keys),
            &answers,
            (request, cross),
        )
        .await
    }

    /// Reads the final Choice's `answers`, asked by `request` over `pool`
    /// under `keys`. A deliberating run settles it on its evidence
    /// (`escalate`), checking it against `cross` when there is one;
    /// otherwise a pick under [`ACT`] is re-asked and corroborated before it
    /// is used.
    async fn settle(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        purpose: &str,
        (pool, keys): (Vec<Candidate>, &[String]),
        answers: &BTreeMap<String, Answer>,
        (request, cross): (EvaluationRequest, Option<Candidate>),
    ) -> Result<Option<Grounded>, Halt> {
        if self.deliberates(FlowLoop::Evidence) {
            let named = chosen(answers, "target")
                .and_then(|(choice, _)| {
                    let index = keys.iter().position(|key| *key == choice)?;
                    pool.get(index)
                })
                .is_some_and(|first| exact_named_match(purpose, Some(first)));
            let offer = Offer {
                screen,
                purpose,
                pool,
                keys: keys.to_vec(),
                request,
                bar: Bar::over(if named { NAMED_FLOOR } else { ACT }),
                cross,
            };
            return self.deliberate_target(log, offer, answers.clone()).await;
        }
        let Some((choice, confidence)) = chosen(answers, "target") else {
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

/// The knockout's group winners, in page order. When the region question
/// chose a region holding at least one winner, only the winners of the
/// `kept` regions go on: its answer is the coarse look a person takes
/// first, and a close second region is kept beside it.
fn winners(
    knockout: &BTreeMap<String, Answer>,
    groups: Vec<(Option<usize>, Vec<Candidate>)>,
    kept: &[usize],
    regions: Option<&(Vec<String>, Regions)>,
) -> Vec<Candidate> {
    let won = groups
        .into_iter()
        .enumerate()
        .filter_map(|(index, (home, group))| {
            let (choice, _) = chosen(knockout, &format!("group_{index}"))?;
            let position = choice.parse::<usize>().ok()?.checked_sub(1)?;
            group.into_iter().nth(position).map(|winner| (home, winner))
        })
        .collect::<Vec<_>>();
    if kept.is_empty() {
        return won.into_iter().map(|(_, winner)| winner).collect();
    }
    let inside = |home: Option<usize>, winner: &Candidate| {
        home.map_or_else(
            || {
                kept.iter().any(|region| {
                    regions
                        .and_then(|(_, regions)| regions.get(*region))
                        .is_some_and(|(_, members)| {
                            members.iter().any(|member| member.ref_id == winner.ref_id)
                        })
                })
            },
            |home| kept.contains(&home),
        )
    };
    if won.iter().any(|(home, winner)| inside(*home, winner)) {
        won.into_iter()
            .filter(|(home, winner)| inside(*home, winner))
            .map(|(_, winner)| winner)
            .collect()
    } else {
        won.into_iter().map(|(_, winner)| winner).collect()
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
