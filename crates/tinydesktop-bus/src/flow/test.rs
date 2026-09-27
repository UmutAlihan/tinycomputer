//! Wire-form tests for intent-flow payloads.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    FLOW_GUIDE, Flow, FlowAction, FlowLoop, FlowStep, FlowStopReason, GroundingHint,
    RunFlowRequest, Slot, Slots, StepOutcome,
};
use serde_json::json;

#[test]
fn a_bare_string_step_is_a_do_intent() {
    let step: FlowStep = serde_json::from_value(json!("start a new note")).unwrap();
    assert_eq!(step, FlowStep::Intent("start a new note".into()));
    assert_eq!(step.action(), FlowAction::Do("start a new note".into()));
    assert_eq!(serde_json::to_value(&step).unwrap(), json!("start a new note"));
}

#[test]
fn enter_keeps_its_slots_in_document_order_when_parsed_from_text() {
    let step: FlowStep = serde_json::from_str(
        r#"{"enter": {"recipient": "a@b.c", "subject": "Hi", "body": "Text"}}"#,
    )
    .unwrap();
    let FlowStep::Action(FlowAction::Enter(Slots(slots))) = &step else {
        panic!("expected an enter step, got {step:?}");
    };
    let order = slots
        .iter()
        .map(|slot| slot.slot.as_str())
        .collect::<Vec<_>>();
    assert_eq!(order, ["recipient", "subject", "body"]);
    assert_eq!(
        serde_json::to_string(&step).unwrap(),
        r#"{"enter":{"recipient":"a@b.c","subject":"Hi","body":"Text"}}"#
    );
    assert!(serde_json::from_value::<Slots>(json!(["not", "a", "map"])).is_err());
}

#[test]
fn malformed_steps_explain_what_is_wrong() {
    let unknown = serde_json::from_value::<FlowStep>(json!({"click": "Send"}))
        .unwrap_err()
        .to_string();
    assert!(unknown.contains("unknown step kind `click`"), "{unknown}");
    let two_keys = serde_json::from_value::<FlowStep>(json!({"do": "a", "verify": "b"}))
        .unwrap_err()
        .to_string();
    assert!(two_keys.contains("exactly one key"), "{two_keys}");
    let wrong_payload = serde_json::from_value::<FlowStep>(json!({"read": "subject"}))
        .unwrap_err()
        .to_string();
    assert!(wrong_payload.contains("in `read` step"), "{wrong_payload}");
    let empty = serde_json::from_value::<FlowStep>(json!({}))
        .unwrap_err()
        .to_string();
    assert!(empty.contains("found none"), "{empty}");
    for value in [json!(null), json!(true), json!(3), json!([])] {
        let error = serde_json::from_value::<FlowStep>(value)
            .unwrap_err()
            .to_string();
        assert!(error.contains("a step: a string"), "{error}");
    }
    let borrowed: FlowStep = serde_json::from_str(r#""plain""#).unwrap();
    assert_eq!(borrowed, FlowStep::Intent("plain".into()));
}

#[test]
fn control_steps_pin_their_wire_form() {
    let flow: Flow = serde_json::from_value(json!({
        "app": "Finder",
        "steps": [
            {"if": {"condition": "c", "then": ["a"], "else": [{"verify": "v"}]}},
            {"repeat_until": {"condition": "done", "steps": ["x"]}},
            {"choose": {"what": "list", "option": "B"}},
            {"read": {"what": "title", "into": "t"}},
            {"wait_for": "ready"},
            {"stop_before": "deleting"},
            {"open": "Finder"},
            {"do": "y"}
        ]
    }))
    .unwrap();
    let FlowStep::Action(FlowAction::RepeatUntil(repeat)) = &flow.steps[1] else {
        panic!("expected repeat_until");
    };
    assert_eq!(repeat.max, 5);
    let round_trip: Flow =
        serde_json::from_value(serde_json::to_value(&flow).unwrap()).unwrap();
    assert_eq!(round_trip, flow);
    assert_eq!(
        serde_json::to_value(&flow).unwrap()["steps"][0]["if"]["else"],
        json!([{"verify": "v"}])
    );
}

#[test]
fn run_requests_default_to_safe_bounded_runs() {
    let request: RunFlowRequest =
        serde_json::from_value(json!({"flow": {"app": "Mail", "steps": ["x"]}})).unwrap();
    assert!(!request.allow_destructive && !request.include_values);
    assert_eq!((request.max_actions, request.max_model_calls), (60, 150));
    assert!(request.disabled_loops.is_empty() && request.memory.is_empty());
    assert_eq!(request, RunFlowRequest {
        flow: request.flow.clone(),
        ..RunFlowRequest::default()
    });
}

#[test]
fn flow_enums_and_hints_pin_their_wire_spelling() {
    assert_eq!(
        serde_json::to_value(FlowStopReason::StoppedBeforeDestructive).unwrap(),
        json!("stopped_before_destructive")
    );
    assert_eq!(
        serde_json::to_value(StepOutcome::AlreadyDone).unwrap(),
        json!("already_done")
    );
    assert_eq!(
        serde_json::to_value(FlowLoop::Corroboration).unwrap(),
        json!("corroboration")
    );
    let hint: GroundingHint = serde_json::from_value(json!({"app": "Mail", "key": "subject"})).unwrap();
    assert!(hint.name.is_none() && hint.path.is_empty());
    let slot = Slot {
        slot: "a".into(),
        text: "b".into(),
    };
    assert_eq!(Slots(vec![slot]).0.len(), 1);
}

#[test]
fn every_example_in_the_guide_is_a_valid_flow() {
    let mut examples = 0;
    for block in FLOW_GUIDE.split("```json").skip(1) {
        let body = block.split("```").next().expect("closed fence");
        let flow: Flow = serde_json::from_str(body)
            .unwrap_or_else(|error| panic!("guide example fails to parse: {error}\n{body}"));
        assert!(!flow.app.is_empty() && !flow.steps.is_empty());
        examples += 1;
    }
    assert!(examples >= 4, "the guide should keep its worked examples");
}
