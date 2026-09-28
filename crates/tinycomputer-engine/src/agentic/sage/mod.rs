//! Levanto Sage as the decision model behind the loops, in place of Jev.
//!
//! The loops ask Jev-shaped requests: one state and a set of yes/no, choice,
//! and score questions. Sage answers the same kinds of question about a
//! document, so a request becomes one Sage batch group:
//!
//! - the state, as compact JSON text, is the group's content;
//! - a yes/no question becomes a Sage `yesno`, and its answer is Sage's
//!   calibrated probability of yes;
//! - a choice becomes a Sage `choice` over the same option names, and its
//!   answer carries Sage's per-option probabilities, normalised to sum to one
//!   (Sage scores each option independently);
//! - a score becomes a Sage five-level `scale`, the request's levels sampled
//!   onto five, and its expectation is spread back over the two nearest of
//!   the request's levels.
//!
//! A verdict Sage calls unsure keeps its probabilities: the loops decide on
//! those, with their own thresholds and deliberation. Every call still goes
//! through [`super::JevRuntime::evaluate`], so budgets, masking, voting, and
//! the journal apply exactly as they do to Jev.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

use serde_json::Value;
use tinyinference_decisions::sage::{
    BatchDecisionRequest, BatchDecisionResponse, BatchGroup, BatchQuestion, ChoiceOption,
    DecisionContent, DecisionQuestion, DecisionResponse, LatencyMode, ReasoningMode, SageClient,
    ScaleLevel,
};
use tinyinference_decisions::{
    Answer, ChoiceAnswer, Error, EvaluationFailure, EvaluationRequest, EvaluationResponse,
    EvaluationResult, NoulAnswer, Question, ScoreAnswer, Usage,
};

/// The levels a Sage scale has, always.
const SCALE_LEVELS: usize = 5;

/// Answers the loops' requests with Sage.
pub(super) struct SageEvaluator {
    client: SageClient,
    latency: LatencyMode,
}

impl SageEvaluator {
    /// An evaluator asking `client`; `fast` scores a choice in one pass
    /// instead of one pass per option.
    pub(super) fn new(client: SageClient, fast: bool) -> Self {
        Self {
            client,
            latency: if fast {
                LatencyMode::Fast
            } else {
                LatencyMode::Quality
            },
        }
    }
}

impl super::Evaluator for SageEvaluator {
    fn evaluate<'a>(
        &'a self,
        request: &'a EvaluationRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EvaluationResult, EvaluationFailure>> + Send + 'a>>
    {
        Box::pin(async move {
            let started = Instant::now();
            let failed = |error: Error, attempts: u32| EvaluationFailure {
                error: Box::new(error),
                attempts,
                latency: started.elapsed(),
            };
            let batch = batch(request, self.latency)
                .map_err(|reason| failed(Error::InvalidRequest { reason }, 0))?;
            let response = self
                .client
                .decide_batch(&batch)
                .await
                .map_err(|error| failed(error, 1))?;
            let answers = answers(request, &response)
                .map_err(|reason| failed(Error::InvalidRequest { reason }, 1))?;
            Ok(EvaluationResult {
                response: EvaluationResponse {
                    model: response.meta.model.clone(),
                    answers,
                    usage: Usage {
                        input_tokens: response
                            .meta
                            .usage
                            .as_ref()
                            .map(|usage| usage.billed_input_tokens),
                        output_tokens: None,
                    },
                },
                request_id: None,
                attempts: 1,
                latency: started.elapsed(),
            })
        })
    }
}

/// `request` as one Sage batch group, its questions in the request's order.
fn batch(
    request: &EvaluationRequest,
    latency: LatencyMode,
) -> Result<BatchDecisionRequest, String> {
    let questions = request
        .questions
        .iter()
        .map(|(id, question)| {
            Ok(BatchQuestion {
                question: sage_question(id, question)?,
                grounding: None,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if questions.is_empty() {
        return Err("a request needs at least one question".to_owned());
    }
    Ok(BatchDecisionRequest {
        requests: vec![BatchGroup {
            content: DecisionContent::Text(
                serde_json::to_string(&request.state).map_err(|error| error.to_string())?,
            ),
            questions,
        }],
        reasoning: ReasoningMode::Auto,
        latency_mode: latency,
    })
}

fn sage_question(id: &str, question: &Question) -> Result<DecisionQuestion, String> {
    Ok(match question {
        Question::Noul(noul) => {
            let mut instructions = text(&noul.instructions);
            if let Some(criteria) = &noul.criteria {
                instructions.push_str(&format!(
                    "\nYes means: {}\nNo means: {}",
                    text(&criteria.r#true),
                    text(&criteria.r#false)
                ));
            }
            DecisionQuestion::YesNo {
                id: id.to_owned(),
                instructions,
            }
        }
        Question::Choice(choice) => {
            if choice.criteria.len() < 2 {
                return Err(format!("choice `{id}` needs at least two options"));
            }
            DecisionQuestion::Choice {
                id: id.to_owned(),
                instructions: text(&choice.instructions),
                options: choice
                    .criteria
                    .iter()
                    .map(|(option, description)| ChoiceOption {
                        option: option.clone(),
                        description: description.as_ref().map(text),
                    })
                    .collect(),
            }
        }
        Question::Score(score) => {
            if score.criteria.len() < 2 {
                return Err(format!("score `{id}` needs at least two levels"));
            }
            DecisionQuestion::Scale {
                id: id.to_owned(),
                instructions: text(&score.instructions),
                levels: (0..SCALE_LEVELS)
                    .map(|level| ScaleLevel {
                        level: u8::try_from(level).unwrap_or(u8::MAX),
                        description: Some(text(
                            &score.criteria[sampled(level, score.criteria.len())],
                        )),
                    })
                    .collect(),
            }
        }
    })
}

/// The request level Sage's `level` (of five) stands for, among `count`.
fn sampled(level: usize, count: usize) -> usize {
    let last = count.saturating_sub(1);
    ((level * last) as f64 / (SCALE_LEVELS - 1) as f64).round() as usize
}

/// Sage's answers as the request's, keyed by its question ids.
fn answers(
    request: &EvaluationRequest,
    response: &BatchDecisionResponse,
) -> Result<BTreeMap<String, Answer>, String> {
    let group = response
        .results
        .first()
        .ok_or_else(|| "Sage returned no answers".to_owned())?;
    if group.answers.len() != request.questions.len() {
        return Err(format!(
            "Sage answered {} of {} questions",
            group.answers.len(),
            request.questions.len()
        ));
    }
    request
        .questions
        .iter()
        .zip(&group.answers)
        .map(|((id, question), answer)| {
            let result = answer
                .result
                .as_ref()
                .filter(|_| answer.ok)
                .ok_or_else(|| {
                    format!(
                        "Sage could not answer `{id}`: {}",
                        answer.error.as_deref().unwrap_or("no reason given")
                    )
                })?;
            Ok((id.clone(), answer_for(id, question, result)?))
        })
        .collect()
}

fn answer_for(id: &str, question: &Question, result: &DecisionResponse) -> Result<Answer, String> {
    Ok(match (question, result) {
        (Question::Noul(_), DecisionResponse::YesNo { result, .. }) => Answer::Noul(NoulAnswer {
            noul: result.probability.clamp(0.0, 1.0),
        }),
        (Question::Choice(choice), DecisionResponse::Choice { result, .. }) => {
            let scored = result
                .probabilities
                .iter()
                .map(|option| (option.option.clone(), option.probability.max(0.0)))
                .collect::<BTreeMap<_, _>>();
            let total = scored.values().sum::<f64>();
            let probabilities = choice
                .criteria
                .keys()
                .map(|option| {
                    let probability = scored.get(option).copied().unwrap_or_default();
                    let share = if total > 0.0 {
                        probability / total
                    } else {
                        0.0
                    };
                    (option.clone(), share)
                })
                .collect::<BTreeMap<_, _>>();
            let leader = probabilities
                .iter()
                .max_by(|left, right| left.1.total_cmp(right.1))
                .map(|(option, _)| option.clone())
                .ok_or_else(|| format!("choice `{id}` came back with no options"))?;
            let chosen = result
                .chosen
                .clone()
                .filter(|chosen| probabilities.contains_key(chosen))
                .unwrap_or(leader);
            Answer::Choice(ChoiceAnswer {
                confidence: probabilities.get(&chosen).copied().unwrap_or_default(),
                choice: chosen,
                probabilities,
            })
        }
        (Question::Score(score), DecisionResponse::Scale { result, .. }) => {
            let last = score.criteria.len().saturating_sub(1);
            let position = (result.expectation.clamp(0.0, (SCALE_LEVELS - 1) as f64) * last as f64)
                / (SCALE_LEVELS - 1) as f64;
            let below = position.floor();
            let above_share = position - below;
            let below = below as usize;
            let probabilities = (0..=last)
                .map(|level| {
                    let probability = if level == below {
                        1.0 - above_share
                    } else if level == below + 1 {
                        above_share
                    } else {
                        0.0
                    };
                    (level.to_string(), probability)
                })
                .collect();
            Answer::Score(ScoreAnswer {
                score: position,
                legend: score
                    .criteria
                    .iter()
                    .enumerate()
                    .map(|(level, description)| (level.to_string(), description.clone()))
                    .collect(),
                probabilities,
                confidence: result.confidence,
            })
        }
        (_, other) => {
            return Err(format!(
                "Sage answered `{id}` as a {}, not the kind asked",
                kind(other)
            ));
        }
    })
}

fn kind(response: &DecisionResponse) -> &'static str {
    match response {
        DecisionResponse::YesNo { .. } => "yes/no",
        DecisionResponse::Choice { .. } => "choice",
        DecisionResponse::Scale { .. } => "scale",
        DecisionResponse::Sort { .. } => "sort",
        DecisionResponse::Tags { .. } => "tags",
    }
}

/// A question field as plain text: a string as is, anything else as JSON.
fn text(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

#[cfg(test)]
mod test;
