//! Grounding: remembered elements, region narrowing, knockouts, re-asks,
//! and the first turn's speculative round.

use super::*;

#[tokio::test]
async fn a_remembered_element_is_confirmed_instead_of_searched_for() {
    let hint = GroundingHint {
        app: "Mail".to_owned(),
        key: "start a new email message".to_owned(),
        role: "button".to_owned(),
        name: Some("New Message".to_owned()),
        path: vec!["window \"Inbox\"".to_owned(), "toolbar".to_owned()],
    };
    let run = run_with(
        App::with(|sim| sim.extra_buttons = 60),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| request.memory = vec![hint],
        |id, question, _| (id == "move").then(|| pick(question, "activate", 0.9)),
    )
    .await;
    assert_eq!(run.app.sim().clicks, ["New Message"]);
    let step = &run.result.steps[0];
    assert!(step.loops.contains(&FlowLoop::Memory));
    assert!(
        !step.loops.contains(&FlowLoop::Narrowing),
        "memory skips the search"
    );
}

#[tokio::test]
async fn a_large_screen_is_narrowed_by_region_before_choosing() {
    let run = run_with(
        App::with(|sim| sim.extra_buttons = 60),
        json!({"app": "Mail", "steps": ["open message 7"]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "region" => Some(pick(question, "Region 1", 0.9)),
            "done" => Some(noul(if sim.clicks.is_empty() { 0.05 } else { 0.9 })),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().clicks, ["Message 7"]);
    assert!(run.result.steps[0].loops.contains(&FlowLoop::Narrowing));
    assert_eq!(
        asked(&run.requests, "region"),
        1,
        "one region question, asked alongside the knockout, not a round per level"
    );
    assert_eq!(asked(&run.requests, "group_0"), 1);
    assert!(
        choice_sizes(&run.requests)
            .iter()
            .all(|size| *size <= ask::CAP + 1)
    );
}

#[tokio::test]
async fn one_crowded_region_falls_back_to_a_knockout() {
    let run = run_with(
        App::with(|sim| {
            sim.extra_buttons = 45;
            sim.quirks.insert(Quirk::OneRegion);
        }),
        json!({"app": "Mail", "steps": ["open message 7"]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "region" => Some(pick(question, "Messages", 0.9)),
            "done" => Some(noul(if sim.clicks.is_empty() { 0.05 } else { 0.9 })),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.app.sim().clicks, ["Message 7"]);
    assert!(
        run.requests
            .iter()
            .any(|request| request.questions.contains_key("group_0")),
        "the knockout asks one question per group"
    );
}

#[tokio::test]
async fn a_low_confidence_choice_is_used_only_when_the_re_ask_agrees() {
    // The legacy re-ask and corroboration path: a deliberating run settles
    // a low pick with its evidence ladder instead (`deliberation_tests.rs`).
    let agreed = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| request.deliberation = Deliberation::Off,
        |id, question, _| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(pick(question, "New Message", 0.5)),
            _ => None,
        },
    )
    .await;
    assert_eq!(agreed.app.sim().clicks, ["New Message"]);
    let loops = &agreed.result.steps[0].loops;
    assert!(loops.contains(&FlowLoop::Consistency) && loops.contains(&FlowLoop::Corroboration));

    let disagreed = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| {
            request.max_actions = 3;
            request.deliberation = Deliberation::Off;
        },
        |id, question, _| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(pick(question, "New Message", 0.5)),
            "again" => Some(pick(question, "Archive", 0.8)),
            "confirm" => Some(noul(0.3)),
            _ => None,
        },
    )
    .await;
    assert!(
        disagreed.app.sim().clicks.is_empty(),
        "disagreement means no click"
    );
    assert_eq!(disagreed.result.stop, FlowStopReason::StepFailed);
}

#[tokio::test]
async fn a_first_turn_asks_the_judge_and_the_target_in_one_round_trip() {
    let app = App::default();
    let scratch = std::env::temp_dir().join(format!(
        "tinycomputer-flow-batch-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let runtime = runtime(Oracle {
        app: app.clone(),
        hook: Box::new(activate_moves),
        requests: Mutex::new(Vec::new()),
        fail: false,
    })
    .with_journal(&scratch);
    let request = RunFlowRequest {
        flow: serde_json::from_value(
            json!({"app": "Mail", "steps": [{"open": "Mail"}, "start a new email message"]}),
        )
        .unwrap(),
        votes: 1,
        ..RunFlowRequest::default()
    };
    let reply = super::run_flow(app.clone(), runtime, request).await;
    assert!(reply.ok, "flow run failed: {:?}", reply.error);
    assert_eq!(app.sim().clicks, ["New Message"]);
    let run = std::fs::read_dir(&scratch)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let journal = std::fs::read_to_string(run.join(crate::JOURNAL_FILE)).unwrap();
    let _ = std::fs::remove_dir_all(&scratch);
    let events = journal
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let turns = events
        .iter()
        .filter(|event| event["event"] == "turn")
        .collect::<Vec<_>>();
    assert_eq!(
        (turns[0]["decisions"].as_u64(), turns[0]["rounds"].as_u64()),
        (Some(2), Some(1)),
        "the judge and the target share the first turn's round trip"
    );
    assert_eq!(
        (turns[1]["decisions"].as_u64(), turns[1]["rounds"].as_u64()),
        (Some(1), Some(1)),
        "after acting, the judge is asked alone"
    );
    let batched = events
        .iter()
        .filter(|event| event["event"] == "decision" && event["batched"] == 2)
        .count();
    assert_eq!(batched, 2, "both decisions of the batch say so");
}
