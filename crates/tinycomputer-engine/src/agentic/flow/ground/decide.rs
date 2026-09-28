//! Grounding's final Choice, re-asked and corroborated when it is not
//! confident, or settled on its evidence by a deliberating run.

use std::collections::BTreeMap;

use serde_json::json;
use tinycomputer_bus::FlowLoop;
use tinyinference_decisions::{Answer, EvaluationRequest};

use crate::agentic::flow::{denoise, escalate::Offer, evidence::Bar};

use crate::agentic::flow::{
    AgentBackend, FlowRun, Halt, StepLog,
    ask::{self, CAP, Questions, chosen, corroborate, elements, lettered, numbered, probability},
    memory::recall,
    view::{ACT, Candidate, Screen, distinct, exact_named_match, label, named_first},
};

use super::{NAMED_FLOOR, CORROBORATED, AGREED, MAX_REGION_DEPTH, BRANCH_MARGIN, Regions, Grounded, Opening, First, winners, split};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {

    /// The final Choice, re-asked and corroborated when it is not confident.
    ///
    /// `wider` is a larger pool the narrowing tree cut `pool` from: a deep
    /// run asks the same Choice over it in the same round trip, and checks
    /// the pick against the one made without the cut.
    pub(in crate::agentic::flow) async fn decide(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        purpose: &str,
        mut pool: Vec<Candidate>,
        wider: Option<Vec<Candidate>>,
    ) -> Result<Option<Grounded>, Halt> {
        pool.truncate(crate::agentic::flow::view::MAX_CANDIDATES);
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
    pub(super) async fn settle(
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
