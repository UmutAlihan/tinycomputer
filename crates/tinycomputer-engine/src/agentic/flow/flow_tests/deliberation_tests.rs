//! Deliberation against the simulator: the evidence gate and the
//! escalation ladder — more framings, duels, contrast, and views
//! (`docs/specs/jev-deliberation.md`).

use super::*;

/// Two result cards whose "Select" buttons look alike: they differ only by
/// the card they sit in.
fn lookalikes() -> App {
    App::with(|sim| {
        sim.results = vec![("IndiGo", "₹5,000", "06:00"), ("IndiGo", "₹5,200", "09:00")];
    })
}

fn shop() -> App {
    App::with(|sim| sim.pages = vec![EXTRAS])
}

fn select_flow() -> Value {
    json!({"app": "Mail", "steps": ["select the 09:00 flight"]})
}

fn loops(run: &Run, index: usize) -> &[FlowLoop] {
    &run.result.steps[index].loops
}

#[tokio::test]
async fn clear_evidence_costs_nothing_more_than_the_legacy_gates() {
    let deep = run(App::default(), mail_flow()).await;
    let off = run_with(
        App::default(),
        mail_flow(),
        |request| request.deliberation = Deliberation::Off,
        |_, _, _| None,
    )
    .await;
    assert_eq!(deep.result.stop, off.result.stop);
    assert_eq!(
        deep.requests.len(),
        off.requests.len(),
        "a decisive oracle is accepted at every gate without a further call"
    );
    let deliberating = |request: &EvaluationRequest| {
        request.questions.keys().any(|id| {
            ["intended", "unintended", "wider", "is_0", "only_near_0"].contains(&id.as_str())
                || id.starts_with("duel_")
        })
    };
    assert!(
        !off.requests.iter().any(deliberating),
        "deliberation off asks exactly what it asked before"
    );
    assert!(loops(&deep, 1).contains(&FlowLoop::Evidence));
    assert!(!loops(&off, 1).contains(&FlowLoop::Evidence));
}

#[tokio::test]
async fn lookalike_buttons_are_resolved_by_a_duel() {
    let run = run_with(
        lookalikes(),
        select_flow(),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(weighted(
                question,
                &[("listitem #1", 0.45), ("listitem #2", 0.45)],
            )),
            _ if id.starts_with("duel_") => Some(pick(question, "listitem #2", 0.8)),
            "done" => Some(noul(if sim.picked.is_empty() { 0.05 } else { 0.95 })),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().picked, ["@s:select-2"]);
    for used in [FlowLoop::Evidence, FlowLoop::Escalation, FlowLoop::Duel] {
        assert!(loops(&run, 0).contains(&used), "{used:?}");
    }
    assert!(asked(&run.requests, "duel_0_1") >= 1 && asked(&run.requests, "duel_1_0") >= 1);
    assert!(
        run.requests
            .iter()
            .filter(|request| request.questions.contains_key("target"))
            .count()
            > 2,
        "the tied Choice was widened into more framings before the duel"
    );
    // `widen` asks Jev outside `FlowRun::ask_batch`, so it must still trace:
    // a widening exchange belongs in the trace beside every other one, or a
    // caller reading `trace` back would see the pre-widening answer even
    // though the widened tally is what actually decided the step.
    assert!(
        run.result
            .trace
            .iter()
            .any(|exchange: &JevExchange| exchange.questions["target"].is_object()),
        "widening a tied target Choice is traced like any other decision"
    );
}

#[tokio::test]
async fn standard_deliberation_takes_the_duel_champion_without_contrast() {
    let run = run_with(
        lookalikes(),
        select_flow(),
        |request| request.deliberation = Deliberation::Standard,
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(weighted(
                question,
                &[("listitem #1", 0.45), ("listitem #2", 0.45)],
            )),
            _ if id.starts_with("duel_") => Some(pick(question, "listitem #2", 0.8)),
            "done" => Some(noul(if sim.picked.is_empty() { 0.05 } else { 0.95 })),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.app.sim().picked, ["@s:select-2"]);
    assert_eq!(asked(&run.requests, "is_0"), 0);
    assert_eq!(
        asked(&run.requests, "intended"),
        0,
        "standard asks only on a miss"
    );
}

#[tokio::test]
async fn a_duel_split_by_position_bias_is_settled_by_contrast() {
    let run = run_with(
        lookalikes(),
        select_flow(),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(weighted(
                question,
                &[("listitem #1", 0.45), ("listitem #2", 0.45)],
            )),
            // Whichever is shown first wins: the pairing comes out even.
            _ if id.starts_with("duel_") => Some(pick(question, "1", 0.8)),
            _ if id.starts_with("is_") => Some(noul(
                if text_of(question, "element").contains("listitem #2") {
                    0.9
                } else {
                    0.2
                },
            )),
            "done" => Some(noul(if sim.picked.is_empty() { 0.05 } else { 0.95 })),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.app.sim().picked, ["@s:select-2"]);
    assert!(
        asked(&run.requests, "is_1") >= 1,
        "both finalists were contrasted"
    );
}

#[tokio::test]
async fn a_close_call_nothing_settles_is_pressed_at_its_best_ranking() {
    let run = run_with(
        lookalikes(),
        select_flow(),
        |request| request.max_actions = 4,
        |id, question, _| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(weighted(
                question,
                &[("listitem #1", 0.45), ("listitem #2", 0.45)],
            )),
            _ if id.starts_with("duel_") => Some(pick(question, "1", 0.8)),
            _ if id.starts_with("is_") => Some(noul(0.5)),
            "done" => Some(noul(0.05)),
            _ => None,
        },
    )
    .await;
    // Pressing nothing would stall the step; the leader is pressed and its
    // effect checked, with the other lookalike kept for a backtrack.
    assert!(
        !run.app.sim().picked.is_empty(),
        "the leader is pressed, not left"
    );
    assert!(loops(&run, 0).contains(&FlowLoop::Duel));
    assert!(
        asked(&run.requests, "is_1") >= 1,
        "the contrast was asked first"
    );
}

#[tokio::test]
async fn escalation_degrades_gracefully_when_the_budget_is_short() {
    let run = run_with(
        lookalikes(),
        select_flow(),
        |request| request.max_model_calls = 2,
        |id, question, _| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(weighted(
                question,
                &[("listitem #1", 0.45), ("listitem #2", 0.45)],
            )),
            "done" => Some(noul(0.05)),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::ModelBudget);
    assert_eq!(
        run.app.sim().picked.len(),
        1,
        "with no calls left to deliberate, the pick it has is acted on"
    );
    assert_eq!(asked_prefix(&run.requests, "duel_"), 0);
}

#[tokio::test]
async fn a_condition_split_across_views_does_not_pass() {
    let flow = json!({"app": "Mail", "steps": [{"verify": "the draft shows the recipient"}]});
    // Near the bar with the history; `view_holds` over the screen alone.
    let split = |view_holds: f64| {
        move |id: &str, question: &Question, _: &Sim| {
            (id == "holds").then(|| {
                noul(if text_of(question, "view").is_empty() {
                    0.62
                } else {
                    view_holds
                })
            })
        }
    };
    let deep = run_with(App::default(), flow.clone(), |_| {}, split(0.3)).await;
    assert_eq!(deep.result.stop, FlowStopReason::StepFailed);
    assert!(loops(&deep, 0).contains(&FlowLoop::Escalation));
    assert!(
        deep.requests.iter().any(|request| request
            .questions
            .get("holds")
            .is_some_and(|question| !text_of(question, "view").is_empty())),
        "the screen-only view was asked"
    );
    let off = run_with(
        App::default(),
        flow.clone(),
        |request| request.deliberation = Deliberation::Off,
        split(0.3),
    )
    .await;
    assert_eq!(
        off.result.stop,
        FlowStopReason::Completed,
        "without deliberation the same answers pass at 0.81"
    );
    let agreed = run_with(App::default(), flow, |_| {}, split(0.95)).await;
    assert_eq!(
        agreed.result.stop,
        FlowStopReason::Completed,
        "views that agree settle it"
    );
}

#[tokio::test]
async fn disabled_elements_never_reach_jev() {
    let run = run_with(
        App::with(|sim| {
            sim.quirks.insert(Quirk::DisabledArchive);
        }),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |_| {},
        |id, question, _| (id == "move").then(|| pick(question, "activate", 0.9)),
    )
    .await;
    assert_eq!(run.app.sim().clicks, ["New Message"]);
    let offered_archive = run.requests.iter().any(|request| {
        request.questions.get("target").is_some_and(|question| {
            let Question::Choice(choice) = question else {
                return false;
            };
            choice
                .criteria
                .values()
                .flatten()
                .any(|description| description.to_string().contains("Archive"))
        })
    });
    assert!(!offered_archive);
    assert!(loops(&run, 0).contains(&FlowLoop::Denoise));
}
