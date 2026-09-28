//! Tests for asking Sage the loops' Jev-shaped requests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;
use tinyinference_decisions::sage::{
    BatchDecisionResponse, DecisionContent, DecisionQuestion, LatencyMode, SageClient,
};
use tinyinference_decisions::{Answer, EvaluationRequest};

use super::{SageEvaluator, answers, batch, sampled};
use crate::agentic::Evaluator;

fn request() -> EvaluationRequest {
    serde_json::from_value(json!({
        "model": "jev-latest",
        "state": {"step": "search for flights", "screen": ["Search", "Close"]},
        "questions": {
            "done": {"type": "noul", "instructions": "Is the search done?",
                     "criteria": {"true": "results are listed", "false": "the form still shows"}},
            "target": {"type": "choice", "instructions": "Which control searches?",
                       "criteria": {"1": "Search", "2": "Close", "none": "nothing here"}},
            "progress": {"type": "score", "instructions": "How far along is the step?",
                         "criteria": ["not started", "half way", "finished"]}
        }
    }))
    .unwrap()
}

fn reply(answers: serde_json::Value) -> BatchDecisionResponse {
    serde_json::from_value(json!({
        "results": [{"answers": answers}],
        "meta": {"model": "levanto-sage-v1.1", "request_count": 1, "question_count": 3,
                 "usage": {"billed_input_tokens": 420, "image_count": 0, "image_tokens": 0}}
    }))
    .unwrap()
}

fn meta() -> serde_json::Value {
    json!({"model": "levanto-sage-v1.1"})
}

#[test]
fn a_request_becomes_one_batch_group_with_a_question_per_question() {
    let batch = batch(&request(), LatencyMode::Fast).unwrap();
    assert_eq!(batch.requests.len(), 1);
    assert_eq!(batch.latency_mode, LatencyMode::Fast);
    let group = &batch.requests[0];
    let DecisionContent::Text(content) = &group.content else {
        panic!("the state is sent as text");
    };
    assert!(content.contains("search for flights"));
    let questions = group
        .questions
        .iter()
        .map(|question| question.question.clone())
        .collect::<Vec<_>>();
    // The request's questions in their (sorted) order: done, progress, target.
    let DecisionQuestion::YesNo { id, instructions } = &questions[0] else {
        panic!("{:?}", questions[0]);
    };
    assert_eq!(id, "done");
    assert!(instructions.contains("Yes means: results are listed"));
    assert!(instructions.contains("No means: the form still shows"));
    let DecisionQuestion::Scale { levels, .. } = &questions[1] else {
        panic!("{:?}", questions[1]);
    };
    let described = levels
        .iter()
        .map(|level| level.description.clone().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        described,
        [
            "not started",
            "half way",
            "half way",
            "finished",
            "finished"
        ],
        "three levels sampled onto Sage's five"
    );
    let DecisionQuestion::Choice { options, .. } = &questions[2] else {
        panic!("{:?}", questions[2]);
    };
    assert_eq!(options.len(), 3);
    assert_eq!(options[0].option, "1");
    assert_eq!(options[0].description.as_deref(), Some("Search"));
    assert_eq!(sampled(4, 7), 6);
    assert_eq!(sampled(0, 7), 0);
}

#[test]
fn requests_sage_cannot_take_are_refused_before_sending() {
    let mut one_option = request();
    one_option.questions = serde_json::from_value(json!({
        "target": {"type": "choice", "instructions": "x", "criteria": {"1": null}}
    }))
    .unwrap();
    assert!(batch(&one_option, LatencyMode::Quality).is_err());
    let mut one_level = request();
    one_level.questions = serde_json::from_value(json!({
        "progress": {"type": "score", "instructions": "x", "criteria": ["only"]}
    }))
    .unwrap();
    assert!(batch(&one_level, LatencyMode::Quality).is_err());
    let mut none = request();
    none.questions.clear();
    assert!(batch(&none, LatencyMode::Quality).is_err());
}

#[test]
fn sage_answers_come_back_as_jev_answers() {
    let response = reply(json!([
        {"ok": true, "result": {"id": "done", "kind": "yesno",
            "result": {"answer": null, "probability": 0.64}, "meta": meta()}},
        {"ok": true, "result": {"id": "progress", "kind": "scale",
            "result": {"expectation": 3.0, "confidence": 0.8}, "meta": meta()}},
        {"ok": true, "result": {"id": "target", "kind": "choice",
            "result": {"chosen": null, "probability": null, "probabilities": [
                {"option": "1", "probability": 0.72},
                {"option": "2", "probability": 0.70},
                {"option": "none", "probability": 0.02}]}, "meta": meta()}}
    ]));
    let answers = answers(&request(), &response).unwrap();
    let Answer::Noul(done) = &answers["done"] else {
        panic!("{:?}", answers["done"]);
    };
    assert!(
        (done.noul - 0.64).abs() < 1e-9,
        "an unsure verdict keeps its probability"
    );
    let Answer::Choice(target) = &answers["target"] else {
        panic!("{:?}", answers["target"]);
    };
    assert_eq!(target.choice, "1", "too close to call: the leader stands");
    let total = target.probabilities.values().sum::<f64>();
    assert!((total - 1.0).abs() < 1e-9, "independent scores normalised");
    assert!((target.probabilities["1"] - 0.72 / 1.44).abs() < 1e-9);
    let Answer::Score(progress) = &answers["progress"] else {
        panic!("{:?}", answers["progress"]);
    };
    assert!(
        (progress.score - 1.5).abs() < 1e-9,
        "3 of 0..4 is 1.5 of 0..2"
    );
    assert!((progress.probabilities["1"] - 0.5).abs() < 1e-9);
    assert!((progress.probabilities["2"] - 0.5).abs() < 1e-9);
    assert_eq!(progress.legend["0"], json!("not started"));
}

#[test]
fn answers_that_do_not_fit_the_request_are_refused() {
    let short = reply(json!([
        {"ok": true, "result": {"id": "done", "kind": "yesno",
            "result": {"answer": "yes", "probability": 0.9}, "meta": meta()}}
    ]));
    assert!(answers(&request(), &short).unwrap_err().contains("1 of 3"));

    let failed = reply(json!([
        {"ok": false, "error": "content too long"},
        {"ok": true, "result": {"id": "progress", "kind": "scale",
            "result": {"expectation": 1.0, "confidence": 0.5}, "meta": meta()}},
        {"ok": true, "result": {"id": "target", "kind": "choice",
            "result": {"chosen": "2", "probability": 0.9, "probabilities": [
                {"option": "2", "probability": 0.9}]}, "meta": meta()}}
    ]));
    assert!(
        answers(&request(), &failed)
            .unwrap_err()
            .contains("content too long")
    );

    let wrong_kind = reply(json!([
        {"ok": true, "result": {"id": "done", "kind": "scale",
            "result": {"expectation": 1.0, "confidence": 0.5}, "meta": meta()}},
        {"ok": true, "result": {"id": "progress", "kind": "scale",
            "result": {"expectation": 1.0, "confidence": 0.5}, "meta": meta()}},
        {"ok": true, "result": {"id": "target", "kind": "choice",
            "result": {"chosen": "2", "probability": 0.9, "probabilities": [
                {"option": "2", "probability": 0.9}]}, "meta": meta()}}
    ]));
    assert!(
        answers(&request(), &wrong_kind)
            .unwrap_err()
            .contains("not the kind asked")
    );

    let empty: BatchDecisionResponse = serde_json::from_value(json!({
        "results": [], "meta": {"model": "m", "request_count": 0, "question_count": 0}
    }))
    .unwrap();
    assert!(answers(&request(), &empty).is_err());
}

#[tokio::test]
async fn an_unreachable_sage_is_a_failure_with_its_attempt() {
    let client = SageClient::with_base_url("key", "http://127.0.0.1:9/").unwrap();
    let failure = SageEvaluator::new(client, true)
        .evaluate(&request())
        .await
        .unwrap_err();
    assert_eq!(failure.attempts, 1);
    assert!(crate::agentic::JevRuntime::sage(" ", false).is_err());
    assert!(crate::agentic::JevRuntime::sage("key", false).is_ok());
}
