//! The wide strategy: one request per turn over a digest of the screen and
//! the run's working memory.

use super::*;

#[tokio::test]
async fn a_wide_turn_judges_and_chooses_its_target_in_one_request() {
    let flow = json!({"app": "Mail", "steps": [{"open": "Mail"}, "start a new email message"]});
    let narrow = run_with(App::default(), flow.clone(), |_| {}, activate_moves).await;
    let broad = run_with(App::default(), flow, wide, activate_moves).await;

    assert_eq!(outcomes(&narrow.result), outcomes(&broad.result));
    assert_eq!(broad.app.sim().clicks, ["New Message"]);
    assert_eq!(
        narrow.result.steps[1].jev_calls, 3,
        "narrow: judge, choose, then judge the result"
    );
    assert_eq!(
        broad.result.steps[1].jev_calls, 2,
        "wide: one request judges the screen and chooses the control, one judges the result"
    );
    let first = &broad.requests[0];
    for id in [
        "done",
        "not_done",
        "progress",
        "move",
        "target_activate",
        "again_activate",
    ] {
        assert!(first.questions.contains_key(id), "the turn asks {id}");
    }
}

#[tokio::test]
async fn a_wide_question_sees_the_screen_as_a_digest_and_the_run_as_memory() {
    let app = App::quirky(Quirk::BodyIgnoresSetValue);
    let run = run_with(app, mail_flow(), wide, |_, _, _| None).await;
    assert_eq!(run.result.stop, FlowStopReason::StoppedBeforeDestructive);

    let entering = run
        .result
        .trace
        .iter()
        .find(|exchange| exchange.step == "3")
        .expect("the enter step asks Jev");
    let state = &entering.state;
    assert!(state.get("recent_actions").is_none());
    assert!(state.get("elements").is_none());
    let regions = &state["screen"]["untrusted_accessibility_data"]["regions"];
    assert!(
        regions
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|region| region["elements"].as_array().unwrap())
            .any(|line| line.as_str().unwrap().contains("textfield \"To\""))
    );
    let memory = &state["memory"];
    assert!(
        memory["steps_done"][1]
            .as_str()
            .unwrap()
            .starts_with("step 2 (do \"start a new email message\"): Done"),
        "{memory}"
    );
    assert_eq!(
        memory["next_step"],
        "verify: the draft shows the recipient, subject and body"
    );
    assert!(memory["budget_left"]["actions"].as_u64().unwrap() > 0);
    for exchange in &run.result.trace {
        assert!(
            !exchange.state["memory"]
                .to_string()
                .contains("Could we move it"),
            "the memory holds what happened, never the text that was typed"
        );
    }
}

#[tokio::test]
async fn a_wide_run_without_the_digest_shows_the_flat_list_with_memory() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| {
            wide(request);
            request.disabled_loops = vec![FlowLoop::Digest];
        },
        |_, _, _| None,
    )
    .await;
    let state = &run.result.trace[0].state;
    assert!(state.get("screen").is_none());
    assert!(state["elements"]["untrusted_accessibility_data"].is_array());
    assert_eq!(state["memory"]["now"], "start a new email message");
}

#[tokio::test]
async fn what_changed_nothing_is_remembered_as_tried_and_failed() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy the inbox"]}),
        wide,
        activate_moves,
    )
    .await;
    assert_eq!(run.result.steps[0].outcome, StepOutcome::Failed);
    let later = run
        .result
        .trace
        .iter()
        .find(|exchange| exchange.state["memory"].get("tried_and_failed").is_some())
        .expect("a later turn is told what did not work");
    assert_eq!(
        later.state["memory"]["tried_and_failed"][0],
        "pressed button \"Archive\": nothing on screen changed"
    );
}

#[tokio::test]
async fn an_obstacle_in_front_is_cleared_from_the_turns_own_request_and_remembered() {
    let app = App::with(|sim| sim.obstacle = true);
    let run = run_with(
        app,
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        wide,
        |_, _, _| None,
    )
    .await;
    assert_eq!(run.result.steps[0].outcome, StepOutcome::Done);
    let clicks = run.app.sim().clicks.clone();
    assert_eq!(clicks, ["Keep Editing"]);
    let Question::Choice(dismiss) = &run.requests[0].questions["dismiss"] else {
        panic!("dismiss is a choice");
    };
    assert!(
        !serde_json::to_string(&dismiss.criteria)
            .unwrap()
            .contains("Delete Draft"),
        "an irreversible control is never offered to dismiss with"
    );
    assert_eq!(asked(&run.requests, "dismiss"), 1);
    assert!(
        run.requests
            .iter()
            .filter(|request| request.questions.contains_key("dismiss"))
            .all(|request| request.questions.contains_key("done")),
        "the obstacle is asked about in the turn's own request, never alone"
    );
    let hint = run
        .result
        .learned
        .iter()
        .find(|hint| hint.key == "obstacle sheet")
        .expect("the control that closed the sheet is remembered");
    assert_eq!(hint.name.as_deref(), Some("Keep Editing"));

    let again = run_with(
        App::with(|sim| sim.obstacle = true),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| {
            wide(request);
            request.memory = vec![hint.clone()];
        },
        // Only the remembered control is confirmed; the chooser picks
        // nothing, so the dismissal can only come from memory.
        |id, question, _| (id == "dismiss").then(|| pick(question, "none", 0.9)),
    )
    .await;
    assert_eq!(asked(&again.requests, "dismiss_known"), 1);
    assert_eq!(again.app.sim().clicks, ["Keep Editing"]);
}

#[tokio::test]
async fn dismiss_options_honor_the_runs_value_visibility_policy() {
    let hidden = run_with(
        App::with(|sim| sim.obstacle = true),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| {
            wide(request);
            request.include_values = false;
        },
        |_, _, _| None,
    )
    .await;
    let Question::Choice(dismiss) = &hidden.requests[0].questions["dismiss"] else {
        panic!("dismiss is a choice");
    };
    assert!(
        !serde_json::to_string(&dismiss.criteria)
            .unwrap()
            .contains("unsaved-draft-42"),
        "a held value is hidden from dismiss options when include_values is false"
    );

    let shown = run_with(
        App::with(|sim| sim.obstacle = true),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| {
            wide(request);
            request.include_values = true;
        },
        |_, _, _| None,
    )
    .await;
    let Question::Choice(dismiss) = &shown.requests[0].questions["dismiss"] else {
        panic!("dismiss is a choice");
    };
    assert!(
        serde_json::to_string(&dismiss.criteria)
            .unwrap()
            .contains("unsaved-draft-42"),
        "a held value must reach dismiss options when the run allows include_values"
    );
}

#[tokio::test]
async fn a_hesitant_wide_pick_is_confirmed_before_it_is_pressed() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        wide,
        |id, question, sim| {
            if id == "target_activate" {
                return Some(pick(question, "New Message", 0.5));
            }
            activate_moves(id, question, sim)
        },
    )
    .await;
    assert_eq!(run.app.sim().clicks, ["New Message"]);
    let confirmations = run
        .requests
        .iter()
        .filter(|request| request.questions.keys().collect::<Vec<_>>() == ["confirm"])
        .count();
    assert_eq!(confirmations, 1, "one yes/no settles the hesitant pick");

    let refused = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| {
            wide(request);
            request.disabled_loops = vec![FlowLoop::Consistency, FlowLoop::Corroboration];
        },
        |id, question, sim| {
            if id == "target_activate" {
                return Some(pick(question, "New Message", 0.5));
            }
            activate_moves(id, question, sim)
        },
    )
    .await;
    assert!(
        refused.app.sim().clicks.is_empty(),
        "with nothing to check a hesitant pick against, it is not pressed"
    );
}

#[tokio::test]
async fn a_journaled_wide_run_records_its_survey_and_each_turns_decisions() {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let app = App::with(|sim| sim.extra_buttons = 60);
    let scratch = std::env::temp_dir().join(format!(
        "tinycomputer-wide-journal-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let runtime = runtime(Oracle {
        app: app.clone(),
        hook: Box::new(|_, _, _| None),
        requests: Mutex::new(Vec::new()),
        fail: false,
    })
    .with_journal(&scratch);
    let request = RunFlowRequest {
        flow: serde_json::from_value(
            json!({"app": "Mail", "steps": ["start a new email message"]}),
        )
        .unwrap(),
        votes: 1,
        strategy: tinycomputer_bus::FlowStrategy::Wide,
        ..RunFlowRequest::default()
    };
    let reply = super::run_flow(app, runtime, request).await;
    assert!(reply.ok, "flow run failed: {:?}", reply.error);
    let run = std::fs::read_dir(&scratch)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let events = std::fs::read_to_string(run.join(crate::JOURNAL_FILE))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    std::fs::remove_dir_all(&scratch).unwrap();
    let survey = events
        .iter()
        .find(|event| event["event"] == "survey")
        .expect("the crowded screen was surveyed");
    assert_eq!(survey["step"], "1");
    assert!(survey["regions"].as_u64().unwrap() >= 2);
    assert!(survey["most_relevant"][0].is_string());
    assert_eq!(survey["distractions"], 0);
    let turns = events
        .iter()
        .filter(|event| event["event"] == "turn")
        .collect::<Vec<_>>();
    assert_eq!(
        turns[0]["decisions"], 2,
        "the first turn: the survey and one wide request"
    );
}

#[tokio::test]
async fn the_memory_keeps_the_last_steps_actions_as_evidence_for_the_next() {
    // Live, a payment page was judged "seat selection skipped" at 0.48
    // until the previous step's clicks were back in view (0.95): the click
    // that left a page is the evidence it was dealt with.
    let flow = json!({"app": "Mail", "steps": [
        "start a new email message",
        {"verify": "a new message is open"}
    ]});
    let run = run_with(App::default(), flow, wide, activate_moves).await;
    let verifying = run
        .result
        .trace
        .iter()
        .find(|exchange| exchange.step == "2")
        .unwrap();
    let recent = verifying.state["memory"]["recent_actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(
        recent
            .iter()
            .any(|line| line.starts_with("click button \"New Message\"")),
        "{recent:?}"
    );
    assert!(
        recent
            .iter()
            .any(|line| line.starts_with("after the last action: window is now \"New Message\"")),
        "{recent:?}"
    );
}

#[tokio::test]
async fn a_target_answered_none_is_not_asked_again_the_same_turn() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        wide,
        |id, question, sim| {
            if id == "target_activate" {
                return Some(pick(question, "none", 0.95));
            }
            activate_moves(id, question, sim)
        },
    )
    .await;
    assert_eq!(asked(&run.requests, "target"), 0, "no narrow re-ask");
    assert!(run.app.sim().clicks.is_empty());
    assert_eq!(run.result.steps[0].outcome, StepOutcome::Failed);
}

#[tokio::test]
async fn a_gated_stop_before_asks_to_find_the_control_not_to_press_it() {
    let run = run(App::default(), mail_flow()).await;
    assert_eq!(run.result.stop, FlowStopReason::StoppedBeforeDestructive);
    let purposes = run
        .requests
        .iter()
        .filter_map(|request| request.questions.get("target"))
        .map(|question| text_of(question, "purpose"))
        .collect::<Vec<_>>();
    assert!(
        purposes.iter().any(|purpose| purpose
            == "find, without pressing it, the control that would perform: sending the email"),
        "{purposes:?}"
    );

    let remembered = run_with(
        App::default(),
        mail_flow(),
        |request| {
            request.memory = vec![GroundingHint {
                app: "Mail".to_owned(),
                key: "perform sending the email".to_owned(),
                role: "button".to_owned(),
                name: Some("Send".to_owned()),
                path: vec!["window \"New Message\"".to_owned(), "toolbar".to_owned()],
            }];
        },
        |_, _, _| None,
    )
    .await;
    assert_eq!(
        remembered.result.stop,
        FlowStopReason::StoppedBeforeDestructive
    );
    assert!(
        remembered
            .requests
            .iter()
            .any(|request| request.questions.keys().collect::<Vec<_>>() == ["confirm"]),
        "a hint stored under the step's own words is still recalled: {:?}",
        remembered
            .requests
            .iter()
            .map(|r| r.questions.keys().cloned().collect::<Vec<_>>())
            .collect::<Vec<_>>()
    );
}

#[test]
fn the_control_pressed_last_turn_is_offered_last_even_with_new_states() {
    let toggle = node(
        "Destination",
        "button",
        &["Click"],
        &["window \"Book\"", "form"],
        10.0,
    );
    let other = node(
        "Srinagar",
        "option",
        &["Click"],
        &["window \"Book\"", "listbox"],
        20.0,
    );
    let mut pool = vec![
        Candidate {
            states: vec!["expanded".to_owned()],
            ..toggle.clone()
        },
        other.clone(),
    ];
    super::wide::pressed_last(&mut pool, Some(&toggle));
    assert_eq!(pool[0].name.as_deref(), Some("Srinagar"));
    let mut grown = vec![
        Candidate {
            name: Some("Destination POPULAR DESTINATIONS Mumbai".to_owned()),
            ..toggle.clone()
        },
        other.clone(),
    ];
    super::wide::pressed_last(&mut grown, Some(&toggle));
    assert_eq!(
        grown[0].name.as_deref(),
        Some("Srinagar"),
        "a button whose name grew by the list it opened is the same button"
    );
    assert_eq!(pool[1].name.as_deref(), Some("Destination"));

    let mut untouched = vec![toggle.clone(), other];
    super::wide::pressed_last(&mut untouched, None);
    assert_eq!(untouched[0].name.as_deref(), Some("Destination"));
}
