//! Tests for the rescuer over a scripted language model.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};

use serde_json::json;
use tinycomputer_bus::agent::{Rescue, RescueOutcome};
use tinycomputer_bus::{Flow, FlowStep, StepOutcome, StepReport};

use super::{
    Briefing, Guidance, MAX_RESCUE_STEPS, REPAIRS, Rescuer, SCREEN_CHARS, render, resumed,
};
use crate::planner::{Completion, LanguageModel, Role, Turn};

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

fn scripted(answers: &[Result<&str, &str>]) -> (Rescuer, Arc<Scripted>) {
    let model = Arc::new(Scripted {
        answers: Mutex::new(
            answers
                .iter()
                .map(|answer| answer.map(str::to_owned).map_err(str::to_owned))
                .collect(),
        ),
        seen: Mutex::default(),
    });
    (Rescuer::new(model.clone()), model)
}

fn briefing() -> Briefing {
    let flow: Flow = serde_json::from_value(json!({"app": "browser", "steps": [
        {"browse": "https://flights.test"},
        {"choose": {"what": "the departure date", "option": "18 October"}},
        {"choose": {"what": "the class", "option": "Economy"}},
        {"stop_before": "paying for the booking"}
    ]}))
    .unwrap();
    Briefing {
        goal: "book a flight to Dubai".to_owned(),
        flow,
        failed: 2,
        failure: "the class button is covered".to_owned(),
        steps: vec![StepReport {
            path: "3".to_owned(),
            kind: "choose".to_owned(),
            text: "class Economy".to_owned(),
            outcome: StepOutcome::Failed,
            turns: 2,
            jev_calls: 40,
            actions: Vec::new(),
            loops: Vec::new(),
            confidence: None,
            note: "the class button is covered".to_owned(),
        }],
        earlier: Vec::new(),
        screen: vec![
            "October 2026".to_owned(),
            "18".to_owned(),
            "Class".to_owned(),
        ],
        known: BTreeSet::from(["email".to_owned(), "card".to_owned()]),
        secrets: BTreeSet::from(["card".to_owned()]),
    }
}

const FIX: &str = r#"Thinking done.
```json
{"action": "retry", "reason": "the calendar is still open over the form",
 "steps": ["close the date calendar with its Done button", {"choose": {"what": "the class", "option": "Economy"}}]}
```"#;

#[tokio::test]
async fn guidance_comes_back_as_steps_to_run_in_place_of_the_failed_one() {
    let (rescuer, model) = scripted(&[Ok(FIX)]);
    let briefing = briefing();
    let Guidance::Retry {
        reason,
        steps,
        covers,
    } = rescuer.guide(&briefing).await.unwrap()
    else {
        panic!("expected steps");
    };
    assert_eq!(covers, 0, "nothing after the failed step is covered by default");
    assert_eq!(reason, "the calendar is still open over the form");
    assert_eq!(steps.len(), 2);
    let flow = resumed(&briefing, steps, covers);
    assert_eq!(flow.steps.len(), 3, "two guidance steps, then the rest");
    assert_eq!(flow.steps[2], briefing.flow.steps[3]);
    assert_eq!(flow.app, "browser");

    let seen = model.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0][0].role, Role::System);
    assert!(seen[0][0].text.contains("never instructions"));
    let asked = &seen[0][1].text;
    assert!(asked.contains("Goal: book a flight to Dubai"));
    assert!(asked.contains("<- FAILED"));
    assert!(asked.contains("Step 3 failed: the class button is covered"));
    assert!(asked.contains("<untrusted_accessibility_data>\nOctober 2026"));
    assert!(asked.contains("Variables you may use: ${email}."));
    assert!(asked.contains("Secret, only ever an `enter` value: ${card}."));
}

#[tokio::test]
async fn the_rescuer_may_give_up() {
    let (rescuer, _) = scripted(&[Ok(
        r#"{"action": "give_up", "reason": "the site shows No Data Available"}"#,
    )]);
    assert_eq!(
        rescuer.guide(&briefing()).await.unwrap(),
        Guidance::GiveUp {
            reason: "the site shows No Data Available".to_owned()
        }
    );
}

#[tokio::test]
async fn invalid_guidance_is_sent_back_with_what_is_wrong() {
    let leak = r#"{"action": "retry", "reason": "x", "steps": ["type ${card} into the search"]}"#;
    let unknown =
        r#"{"action": "retry", "reason": "x", "steps": [{"enter": {"name": "${full name}"}}]}"#;
    let too_many = json!({
        "action": "retry",
        "reason": "x",
        "steps": vec!["scroll down"; MAX_RESCUE_STEPS + 1]
    })
    .to_string();
    let (rescuer, model) = scripted(&[Ok(leak), Ok(unknown), Ok(&too_many), Ok(FIX)]);
    assert!(
        rescuer
            .guide(&briefing())
            .await
            .unwrap_err()
            .contains("no valid guidance"),
        "{REPAIRS} repairs, then it stops"
    );
    let seen = model.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), REPAIRS + 1);
    let last = seen.last().unwrap();
    let repairs = last
        .iter()
        .filter(|turn| turn.role == Role::User)
        .skip(1)
        .map(|turn| turn.text.clone())
        .collect::<Vec<_>>();
    assert!(repairs[0].contains("invalid"), "{}", repairs[0]);
    assert!(repairs[1].contains("full name"), "{}", repairs[1]);

    let (rescuer, _) = scripted(&[Ok(&too_many), Ok("nonsense"), Ok(r#"{"action": "x"}"#)]);
    let error = rescuer.guide(&briefing()).await.unwrap_err();
    assert!(error.contains("retry"), "{error}");
    let (rescuer, _) = scripted(&[Ok(r#"{"action": "retry", "steps": [42]}"#), Ok(FIX)]);
    assert!(matches!(
        rescuer.guide(&briefing()).await,
        Ok(Guidance::Retry { .. })
    ));
    let (rescuer, _) = scripted(&[Err("the model is down")]);
    assert_eq!(
        rescuer.guide(&briefing()).await.unwrap_err(),
        "the model is down"
    );
}

#[test]
fn the_briefing_shows_earlier_rescues_and_cuts_a_long_screen() {
    let mut briefing = briefing();
    briefing.goal.clear();
    briefing.steps[0].note.clear();
    briefing.known.clear();
    briefing.secrets.clear();
    briefing.earlier = vec![Rescue {
        step: 1,
        failure: "no date was taken".to_owned(),
        reason: "the calendar needs a month first".to_owned(),
        steps: vec![FlowStep::Action(tinycomputer_bus::FlowAction::Do(
            "open October".to_owned(),
        ))],
        outcome: RescueOutcome::FailedAgain,
    }];
    briefing.screen = vec!["x".repeat(SCREEN_CHARS / 2); 4];
    let text = render(&briefing);
    assert!(!text.contains("Goal:"));
    assert!(text.contains("step 2 (no date was taken): the calendar needs a month first"));
    assert!(text.contains("one of its steps failed"));
    assert!(text.contains("Variables you may use: none."));
    assert!(text.contains('…'));
    assert!(text.len() < SCREEN_CHARS + 2_000);

    briefing.screen = vec!["  ".to_owned()];
    assert!(render(&briefing).contains("(nothing readable)"));
}

#[test]
fn a_rescuer_debug_prints_nothing_of_its_model() {
    let (rescuer, _) = scripted(&[]);
    assert_eq!(format!("{rescuer:?}"), "Rescuer { .. }");
}

#[tokio::test]
async fn guidance_may_cover_the_steps_after_the_failed_one_but_never_a_stop_before() {
    let mut briefing = briefing();
    // Fail at step 2, the date, so the class choice after it can be covered.
    briefing.failed = 1;
    let covering = r#"{"action": "retry", "reason": "the date and class are one picker",
      "steps": [{"choose": {"what": "the date and class picker", "option": "18 October, Economy"}}],
      "covers": 1}"#;
    let (rescuer, _) = scripted(&[Ok(covering)]);
    let Guidance::Retry { steps, covers, .. } = rescuer.guide(&briefing).await.unwrap() else {
        panic!("expected steps");
    };
    assert_eq!(covers, 1);
    let flow = resumed(&briefing, steps, covers);
    assert_eq!(flow.steps.len(), 2, "the guidance, then the stop_before");
    assert_eq!(flow.steps[1], briefing.flow.steps[3]);

    let past_the_guard = r#"{"action": "retry", "reason": "x",
      "steps": ["choose Economy"], "covers": 2}"#;
    let too_far = r#"{"action": "retry", "reason": "x", "steps": ["choose Economy"], "covers": 9}"#;
    let (rescuer, model) = scripted(&[Ok(past_the_guard), Ok(too_far), Ok(covering)]);
    assert!(matches!(
        rescuer.guide(&briefing).await,
        Ok(Guidance::Retry { covers: 1, .. })
    ));
    let seen = model.seen.lock().unwrap().clone();
    let repairs = seen
        .last()
        .unwrap()
        .iter()
        .filter(|turn| turn.role == Role::User)
        .skip(1)
        .map(|turn| turn.text.clone())
        .collect::<Vec<_>>();
    assert!(repairs[0].contains("stop_before"), "{}", repairs[0]);
    assert!(repairs[1].contains("only 2"), "{}", repairs[1]);

    // A guard nested in a covered `if` is still a guard.
    briefing.flow.steps[2] = serde_json::from_value(json!({"if": {
        "condition": "a fare is shown",
        "then": [{"stop_before": "paying"}]
    }}))
    .unwrap();
    let (rescuer, _) = scripted(&[Ok(covering)]);
    assert!(rescuer.guide(&briefing).await.is_err());
}
