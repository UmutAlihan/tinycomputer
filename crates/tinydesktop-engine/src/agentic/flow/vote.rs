//! Voting: one decision asked several ways, the answers averaged.
//!
//! Jev is cheap and fast, so a flow buys accuracy with calls. Jev takes no
//! temperature and no sample count, so the variety has to come from how a
//! question is framed. Each framing of a request:
//!
//! - presents every Choice whose keys are mere labels (`1`, `2`, … or `A`,
//!   `B`, …) in a different order, under a different style of key (`01`…,
//!   `A`…), which undoes any bias toward the first option or a particular
//!   label. A Choice keyed by meaningful words (`shortcut`, `new_item`) keeps
//!   its keys, since the word is part of what is asked;
//! - adds a short perspective to every question's instructions, so the same
//!   question is read with a different emphasis.
//!
//! The framings run concurrently. Their answers are mapped back to the
//! original keys and averaged per question: a Choice by its per-option
//! probabilities (its `confidence` becomes the share of framings that agreed
//! with the winner), a Noul by its probability, a Score by its per-level
//! probabilities. Framing 0 is always the request as built, so voting once
//! is exactly asking once.

use std::collections::BTreeMap;

use serde_json::Value;
use tinyinference_decisions::{Answer, ChoiceAnswer, EvaluationRequest, NoulAnswer, Question};

/// Most framings one decision is asked in.
pub(super) const MAX_VOTES: u32 = 9;

/// A perspective added to each framing after the first, in turn.
const PERSPECTIVES: [&str; 4] = [
    "Answer as a careful person checking the screen before they click.",
    "Look for concrete visible evidence; do not assume anything that is not shown.",
    "Think about what the whole task needs, not only this one step.",
    "Be strict: when two readings are possible, prefer the one the screen supports best.",
];

/// One framing of a request, and how to read its answers back.
#[derive(Debug, Clone)]
pub(super) struct Framing {
    pub(super) request: EvaluationRequest,
    /// Per Choice question: the framing's key for each original key.
    keys: BTreeMap<String, BTreeMap<String, String>>,
}

/// `count` framings of `request`, the first being `request` itself.
pub(super) fn framings(request: &EvaluationRequest, count: u32) -> Vec<Framing> {
    (0..count.clamp(1, MAX_VOTES))
        .map(|index| frame(request, index as usize))
        .collect()
}

fn frame(request: &EvaluationRequest, index: usize) -> Framing {
    let mut framed = request.clone();
    let mut keys = BTreeMap::new();
    if index == 0 {
        return Framing {
            request: framed,
            keys,
        };
    }
    let perspective = PERSPECTIVES[(index - 1) % PERSPECTIVES.len()];
    for (id, question) in &mut framed.questions {
        match question {
            Question::Choice(choice) => {
                add_perspective(&mut choice.instructions, perspective);
                if labelled(&choice.criteria) {
                    let (criteria, mapping) = reorder(&choice.criteria, index);
                    choice.criteria = criteria;
                    keys.insert(id.clone(), mapping);
                }
            }
            Question::Noul(noul) => add_perspective(&mut noul.instructions, perspective),
            Question::Score(score) => add_perspective(&mut score.instructions, perspective),
        }
    }
    Framing {
        request: framed,
        keys,
    }
}

fn add_perspective(instructions: &mut Value, perspective: &str) {
    if let Value::Object(fields) = instructions {
        fields.insert("perspective".to_owned(), Value::from(perspective));
    }
}

/// Whether every key but `none` is a bare label — digits, or capital
/// letters — that can be swapped for another without changing the question.
fn labelled(criteria: &BTreeMap<String, Option<Value>>) -> bool {
    criteria.keys().filter(|key| *key != "none").all(|key| {
        !key.is_empty()
            && (key.chars().all(|character| character.is_ascii_digit())
                || key.chars().all(|character| character.is_ascii_uppercase()))
    })
}

/// `criteria` presented in framing `index`'s order and key style. `none`
/// keeps its key; the other options are rotated, reversed on odd framings,
/// and keyed so that the keys sort in the new order.
fn reorder(
    criteria: &BTreeMap<String, Option<Value>>,
    index: usize,
) -> (BTreeMap<String, Option<Value>>, BTreeMap<String, String>) {
    let mut options = criteria
        .keys()
        .filter(|key| *key != "none")
        .cloned()
        .collect::<Vec<_>>();
    if !options.is_empty() {
        let shift = (index * options.len()).div_ceil(MAX_VOTES as usize) % options.len();
        options.rotate_left(shift);
    }
    if index % 2 == 1 {
        options.reverse();
    }
    let labels = keys_for(options.len(), index);
    let mut framed = BTreeMap::new();
    let mut mapping = BTreeMap::new();
    for (original, label) in options.iter().zip(labels) {
        framed.insert(label.clone(), criteria[original].clone());
        mapping.insert(original.clone(), label);
    }
    if let Some(none) = criteria.get("none") {
        framed.insert("none".to_owned(), none.clone());
        mapping.insert("none".to_owned(), "none".to_owned());
    }
    (framed, mapping)
}

/// `count` keys that sort in order: zero-padded numbers on even framings,
/// fixed-width capital letters on odd ones.
fn keys_for(count: usize, index: usize) -> Vec<String> {
    if index % 2 == 0 {
        let width = count.to_string().len();
        return (1..=count).map(|key| format!("{key:0width$}")).collect();
    }
    let width = if count <= 26 { 1 } else { 2 };
    (0..count)
        .map(|mut position| {
            let mut key = vec![b'A'; width];
            for slot in key.iter_mut().rev() {
                *slot = b'A' + u8::try_from(position % 26).unwrap_or(0);
                position /= 26;
            }
            String::from_utf8(key).unwrap_or_default()
        })
        .collect()
}

/// The answers of every framing that came back, merged per question under
/// the original keys.
pub(super) fn merge(answered: &[(Framing, BTreeMap<String, Answer>)]) -> BTreeMap<String, Answer> {
    let Some((first, _)) = answered.first() else {
        return BTreeMap::new();
    };
    first
        .request
        .questions
        .keys()
        .filter_map(|id| {
            let answers = answered
                .iter()
                .filter_map(|(framing, answers)| {
                    Some(original(framing, id, answers.get(id)?.clone()))
                })
                .collect::<Vec<_>>();
            Some((id.clone(), average(&answers)?))
        })
        .collect()
}

/// A framing's answer with its Choice keys mapped back to the original ones.
fn original(framing: &Framing, id: &str, answer: Answer) -> Answer {
    let Some(mapping) = framing.keys.get(id) else {
        return answer;
    };
    let Answer::Choice(choice) = answer else {
        return answer;
    };
    let back = mapping
        .iter()
        .map(|(original, framed)| (framed.clone(), original.clone()))
        .collect::<BTreeMap<_, _>>();
    let rename = |key: &String| back.get(key).cloned().unwrap_or_else(|| key.clone());
    Answer::Choice(ChoiceAnswer {
        choice: rename(&choice.choice),
        probabilities: choice
            .probabilities
            .iter()
            .map(|(key, probability)| (rename(key), *probability))
            .collect(),
        confidence: choice.confidence,
    })
}

/// The mean of `answers`, all to one question; `None` when there are none.
fn average(answers: &[Answer]) -> Option<Answer> {
    let first = answers.first()?;
    let count = f64::from(u32::try_from(answers.len()).unwrap_or(u32::MAX));
    Some(match first {
        Answer::Noul(_) => Answer::Noul(NoulAnswer {
            noul: answers
                .iter()
                .filter_map(|answer| match answer {
                    Answer::Noul(noul) => Some(noul.noul),
                    _ => None,
                })
                .sum::<f64>()
                / count,
        }),
        Answer::Choice(_) => {
            let choices = answers
                .iter()
                .filter_map(|answer| match answer {
                    Answer::Choice(choice) => Some(choice),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let probabilities =
                mean_probabilities(choices.iter().map(|choice| &choice.probabilities), count);
            let winner = probabilities
                .iter()
                .max_by(|left, right| left.1.total_cmp(right.1))
                .map(|(key, _)| key.clone())
                .unwrap_or_else(|| "none".to_owned());
            let agreeing = choices
                .iter()
                .filter(|choice| choice.choice == winner)
                .count();
            Answer::Choice(ChoiceAnswer {
                confidence: f64::from(u32::try_from(agreeing).unwrap_or(u32::MAX)) / count,
                choice: winner,
                probabilities,
            })
        }
        Answer::Score(score) => {
            let scores = answers
                .iter()
                .filter_map(|answer| match answer {
                    Answer::Score(score) => Some(score),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let mut merged = score.clone();
            merged.probabilities =
                mean_probabilities(scores.iter().map(|score| &score.probabilities), count);
            merged.score = scores.iter().map(|score| score.score).sum::<f64>() / count;
            merged.confidence = scores.iter().map(|score| score.confidence).sum::<f64>() / count;
            Answer::Score(merged)
        }
    })
}

fn mean_probabilities<'a>(
    all: impl Iterator<Item = &'a BTreeMap<String, f64>>,
    count: f64,
) -> BTreeMap<String, f64> {
    let mut sums = BTreeMap::<String, f64>::new();
    for probabilities in all {
        for (key, probability) in probabilities {
            *sums.entry(key.clone()).or_default() += probability;
        }
    }
    sums.into_iter()
        .map(|(key, sum)| (key, sum / count))
        .collect()
}
