//! Tests for the shaper over a scripted language model, and for the schema
//! subset it checks results against.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tinycomputer_bus::agent::TaskOutput;

use super::{Harvest, RECORDS_CHARS, REPAIRS, Shaper, render, schema};
use crate::planner::{Completion, LanguageModel, Turn};

/// Answers from a queue and records every conversation it was shown.
#[derive(Default)]
struct Scripted {
    answers: Mutex<VecDeque<Result<String, String>>>,
    seen: Mutex<Vec<Vec<Turn>>>,
}

impl LanguageModel for Scripted {
    fn complete(&self, turns: &[Turn]) -> Completion {
        self.seen.lock().unwrap().push(turns.to_vec());
        let answer = self
            .answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err("no more answers".to_owned()));
        Box::pin(async move { answer })
    }
}

fn scripted(answers: &[Result<&str, &str>]) -> (Shaper, Arc<Scripted>) {
    let model = Arc::new(Scripted {
        answers: Mutex::new(
            answers
                .iter()
                .map(|answer| answer.map(str::to_owned).map_err(str::to_owned))
                .collect(),
        ),
        seen: Mutex::default(),
    });
    (Shaper::new(model.clone()), model)
}

fn chats_schema() -> Value {
    json!({
        "type": "object",
        "required": ["chats"],
        "additionalProperties": false,
        "properties": {"chats": {
            "type": "array",
            "maxItems": 2,
            "items": {
                "type": "object",
                "required": ["name", "messages"],
                "properties": {
                    "name": {"type": "string"},
                    "messages": {"type": "array", "items": {"type": "string"}, "maxItems": 3}
                }
            }
        }}
    })
}

fn harvest() -> Harvest {
    Harvest {
        goal: "read my two newest chats".to_owned(),
        output: TaskOutput {
            instructions: "each chat's name and its newest messages".to_owned(),
            schema: Some(chats_schema()),
        },
        reads: vec![
            ("chat_1".to_owned(), "Sam".to_owned()),
            (
                "messages_1".to_owned(),
                json!([["message, hi, 9:00"], ["message, lunch?, 9:05"]]).to_string(),
            ),
        ],
    }
}

#[tokio::test]
async fn a_result_that_fits_the_schema_is_returned() {
    let (shaper, model) = scripted(&[Ok(
        "```json\n{\"chats\": [{\"name\": \"Sam\", \"messages\": [\"hi\", \"lunch?\"]}]}\n```",
    )]);
    let result = shaper.shape(&harvest()).await.unwrap();
    assert_eq!(result["chats"][0]["messages"][1], "lunch?");
    let seen = model.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    let brief = &seen[0][1].text;
    assert!(brief.contains("read my two newest chats"), "{brief}");
    assert!(brief.contains("untrusted_accessibility_data"), "{brief}");
    // An extract's rows travel as rows, not as a string holding JSON.
    assert!(brief.contains("[[\"message, hi, 9:00\"]"), "{brief}");
}

#[tokio::test]
async fn a_result_that_breaks_the_schema_is_sent_back_with_what_is_wrong() {
    let (shaper, model) = scripted(&[
        Ok("{\"chats\": [{\"name\": \"Sam\"}], \"extra\": 1}"),
        Ok("{\"chats\": [{\"name\": \"Sam\", \"messages\": [\"hi\"]}]}"),
    ]);
    let result = shaper.shape(&harvest()).await.unwrap();
    assert_eq!(result["chats"][0]["messages"][0], "hi");
    let seen = model.seen.lock().unwrap();
    let repair = &seen[1].last().unwrap().text;
    assert!(repair.contains("missing `messages`"), "{repair}");
    assert!(repair.contains("`extra`"), "{repair}");
}

#[tokio::test]
async fn a_result_still_broken_after_the_repairs_is_an_error() {
    let broken = "[1, 2]";
    let answers = vec![Ok(broken); REPAIRS + 1];
    let (shaper, model) = scripted(&answers);
    let error = shaper.shape(&harvest()).await.unwrap_err();
    assert!(error.contains("did not fit the requested shape"), "{error}");
    assert_eq!(model.seen.lock().unwrap().len(), REPAIRS + 1);

    let (shaper, _) = scripted(&[Err("the model is down")]);
    assert_eq!(
        shaper.shape(&harvest()).await.unwrap_err(),
        "the model is down"
    );
}

#[tokio::test]
async fn without_a_schema_any_object_is_the_result() {
    let mut harvest = harvest();
    harvest.output.schema = None;
    let (shaper, _) = scripted(&[Ok("{\"anything\": true}")]);
    assert_eq!(
        shaper.shape(&harvest).await.unwrap(),
        json!({"anything": true})
    );
}

#[test]
fn records_past_the_cap_are_left_out_and_said_to_be() {
    let mut harvest = harvest();
    harvest
        .reads
        .push(("huge".to_owned(), "x".repeat(RECORDS_CHARS)));
    let brief = render(&harvest);
    assert!(brief.contains("Some records were left out"), "{brief}");
    assert!(!brief.contains(&"x".repeat(100)));
    assert!(brief.contains("Sam"));
}

#[test]
fn the_schema_subset_is_enforced_up_front() {
    assert!(schema::supported(&chats_schema()).is_ok());
    for (schema, problem) in [
        (json!({"type": "array"}), "top-level"),
        (json!({"type": "object", "pattern": "x"}), "`pattern`"),
        (
            json!({"properties": {"a": {"type": "date"}}}),
            "the schema.a has a `type`",
        ),
        (json!({"required": "a"}), "`required`"),
        (json!({"additionalProperties": {}}), "true or false"),
        (json!({"properties": {"a": {"enum": "x"}}}), "`enum`"),
        (json!({"properties": {"a": {"maxItems": -1}}}), "`maxItems`"),
        (json!({"properties": []}), "`properties`"),
        (json!({"items": 3}), "must be an object"),
    ] {
        let error = schema::supported(&schema).unwrap_err();
        assert!(error.contains(problem), "{schema}: {error}");
    }
}

#[test]
fn violations_name_every_broken_rule_by_where_it_is() {
    let schema = json!({"type": "object", "properties": {
        "count": {"type": "integer"},
        "kind": {"enum": ["a", "b"]},
        "tags": {"type": ["array", "null"], "minItems": 2, "items": {"type": "string"}},
        "flag": {"type": "boolean"}
    }});
    let found = schema::violations(
        &json!({"count": 1.5, "kind": "c", "tags": [1], "flag": null}),
        &schema,
    );
    assert_eq!(found.len(), 5, "{found:?}");
    assert!(
        found
            .iter()
            .any(|v| v.contains("the result.count must be of type"))
    );
    assert!(
        found
            .iter()
            .any(|v| v.contains("the result.kind must be one of"))
    );
    assert!(found.iter().any(|v| v.contains("at least 2 items")));
    assert!(found.iter().any(|v| v.contains("the result.tags[0]")));
    assert!(found.iter().any(|v| v.contains("not null")));
    assert!(schema::violations(&json!({"tags": null, "count": 3}), &schema).is_empty());
}
