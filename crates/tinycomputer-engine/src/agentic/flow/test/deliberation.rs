//! Deliberation against the simulator: the evidence gate, the escalation
//! ladder, tree grounding, denoising, expectations, verified undo, and
//! backtracking (`docs/specs/jev-deliberation.md`).

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
    assert_eq!(run.app.sim().picked.len(), 1);
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

#[tokio::test]
async fn an_oscillation_bans_the_pair() {
    let run = run_with(
        App::with(|sim| sim.trip = Some(("One way", 0))),
        json!({"app": "Mail", "steps": ["show multi-city trips"]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(pick(
                question,
                if sim.clicks.len() == 1 {
                    "One way"
                } else {
                    "Return"
                },
                0.9,
            )),
            "done" => Some(noul(0.05)),
            _ => None,
        },
    )
    .await;
    assert_eq!(
        run.app.sim().clicks,
        ["Return", "One way"],
        "the press that would go back again is never made"
    );
    assert!(loops(&run, 0).contains(&FlowLoop::Denoise));
}

#[tokio::test]
async fn a_wrong_navigation_is_undone_by_going_back_and_verified() {
    let run = run_with(
        shop(),
        json!({"app": "Shop", "steps": ["add travel insurance"]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(pick(
                question,
                if sim.backs == 0 {
                    "Insurance terms"
                } else {
                    "Travel insurance"
                },
                0.9,
            )),
            "helped" => Some(noul(if sim.page() == Some(TERMS) { 0.1 } else { 0.9 })),
            "done" => Some(noul(if sim.checked.contains(INSURANCE) {
                0.95
            } else {
                0.05
            })),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    let sim = run.app.sim();
    assert_eq!(sim.clicks, ["Insurance terms", "Travel insurance"]);
    assert_eq!(sim.backs, 1);
    assert_eq!(sim.page(), Some(EXTRAS));
    for used in [FlowLoop::Expectation, FlowLoop::Checkpoint, FlowLoop::Undo] {
        assert!(loops(&run, 0).contains(&used), "{used:?}");
    }
}

#[tokio::test]
async fn an_unverifiable_undo_fails_the_step_closed() {
    let run = run_with(
        App::with(|sim| {
            sim.pages = vec![EXTRAS];
            sim.quirks.insert(Quirk::StuckHistory);
        }),
        json!({"app": "Shop", "steps": ["add travel insurance"]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(pick(question, "Insurance terms", 0.9)),
            "helped" => Some(noul(if sim.page() == Some(TERMS) { 0.1 } else { 0.9 })),
            "done" => Some(noul(0.05)),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    let step = &run.result.steps[0];
    assert!(
        step.note.contains("does not match where it started"),
        "{}",
        step.note
    );
    assert_eq!(run.app.sim().clicks, ["Insurance terms"]);
    assert_eq!(
        run.app.sim().navigated,
        [EXTRAS],
        "going back was tried, then the address"
    );
}

#[tokio::test]
async fn a_wrong_toggle_is_pressed_again_and_the_runner_up_is_tried() {
    let run = run_with(
        shop(),
        json!({"app": "Shop", "steps": ["add travel insurance"]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(weighted(
                question,
                &[("Seat protection", 0.8), ("Travel insurance", 0.15)],
            )),
            "helped" => Some(noul(if sim.checked.contains(PROTECTION) {
                0.1
            } else {
                0.9
            })),
            "done" => Some(noul(if sim.checked.contains(INSURANCE) {
                0.95
            } else {
                0.05
            })),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    let sim = run.app.sim();
    assert_eq!(
        sim.clicks,
        ["Seat protection", "Seat protection", "Travel insurance"]
    );
    assert!(sim.checked.contains(INSURANCE) && !sim.checked.contains(PROTECTION));
    assert_eq!(
        sim.backs, 0,
        "a toggle is undone in place, not by going back"
    );
    assert!(loops(&run, 0).contains(&FlowLoop::Backtrack));
}

#[tokio::test]
async fn an_irreversible_press_needs_a_deep_accept() {
    let flow = json!({"app": "Mail", "steps": [{"stop_before": "sending the email"}]});
    let unsure = run_with(
        App::with(|sim| sim.compose_open = true),
        flow.clone(),
        |request| request.allow_destructive = true,
        |id, question, _| match id {
            "target" => Some(pick(question, "Send", 0.75)),
            "is_0" => Some(noul(0.6)),
            _ => None,
        },
    )
    .await;
    assert!(!unsure.app.sim().sent, "never sent on uncertain evidence");
    assert_eq!(unsure.result.stop, FlowStopReason::StepFailed);
    assert!(unsure.result.steps[0].note.contains("uncertain evidence"));

    let sure = run_with(
        App::with(|sim| sim.compose_open = true),
        flow,
        |request| request.allow_destructive = true,
        |id, question, _| match id {
            "target" => Some(pick(question, "Send", 0.75)),
            "is_0" => Some(noul(0.97)),
            _ => None,
        },
    )
    .await;
    assert!(sure.app.sim().sent);
    assert_eq!(sure.result.stop, FlowStopReason::Completed);
}

/// A large screen whose target, Message 7, sits in Region 1; each region's
/// knockout finds its own best guess.
fn crowded() -> App {
    App::with(|sim| sim.extra_buttons = 60)
}

fn knockout(question: &Question) -> Answer {
    let seven = pick(question, "Message 7", 0.9);
    match &seven {
        Answer::Choice(choice) if choice.choice != "none" => seven,
        _ => pick(question, "Message 6", 0.9),
    }
}

#[tokio::test]
async fn the_tree_keeps_a_close_second_region() {
    let hook = |id: &str, question: &Question, sim: &Sim| match id {
        "move" => Some(pick(question, "activate", 0.9)),
        "region" => Some(weighted(question, &[("Region 0", 0.5), ("Region 1", 0.4)])),
        _ if id.starts_with("group_") => Some(knockout(question)),
        "target" => Some(pick(question, "Message 7", 0.9)),
        "done" => Some(noul(if sim.clicks.is_empty() { 0.05 } else { 0.95 })),
        _ => None,
    };
    let deep = run_with(
        crowded(),
        json!({"app": "Mail", "steps": ["open message 7"]}),
        |_| {},
        hook,
    )
    .await;
    assert_eq!(deep.app.sim().clicks, ["Message 7"]);
    assert!(loops(&deep, 0).contains(&FlowLoop::TreeGrounding));
    let off = run_with(
        crowded(),
        json!({"app": "Mail", "steps": ["open message 7"]}),
        |request| {
            request.deliberation = Deliberation::Off;
            request.max_actions = 3;
        },
        hook,
    )
    .await;
    assert!(
        !off.app.sim().clicks.contains(&"Message 7".to_owned()),
        "following the close region alone loses the target"
    );
}

#[tokio::test]
async fn a_region_cut_that_lost_the_target_is_caught_by_the_wider_choice() {
    let run = run_with(
        crowded(),
        json!({"app": "Mail", "steps": ["open message 7"]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "region" => Some(weighted(question, &[("Region 0", 0.9)])),
            _ if id.starts_with("group_") => Some(knockout(question)),
            // Offered only the wrong region's winner, the Choice takes it.
            "target" => Some(pick(question, "Message 6", 0.9)),
            "wider" => Some(pick(question, "Message 7", 0.9)),
            _ if id.starts_with("duel_") => Some(pick(question, "Message 7", 0.8)),
            "done" => Some(noul(if sim.clicks.is_empty() { 0.05 } else { 0.95 })),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.app.sim().clicks, ["Message 7"]);
    assert!(asked(&run.requests, "wider") >= 1);
    assert!(loops(&run, 0).contains(&FlowLoop::Duel));
}

#[tokio::test]
async fn views_never_pull_a_judgement_below_the_bar_lower() {
    // Live on IndiGo: after "Next" the passenger form showed, the judge read
    // 0.64 with Jev choosing "finished"; the screen alone read 0.27 and, as a
    // veto, overruled the finish and pressed the form's own Next. Views guard
    // a pass, so a judgement under the bar is left as the judge read it.
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(
                question,
                if sim.clicks.is_empty() {
                    "activate"
                } else {
                    "finished"
                },
                0.9,
            )),
            "done" if !text_of(question, "view").is_empty() => Some(noul(0.2)),
            "done" => Some(noul(if sim.clicks.is_empty() { 0.05 } else { 0.64 })),
            // Nearly there, and only 0.64 on "fully": the judge reads 0.64.
            "progress" if !sim.clicks.is_empty() => Some(Answer::Score(ScoreAnswer {
                score: 3.64,
                legend: BTreeMap::new(),
                probabilities: [("3", 0.36), ("4", 0.64)]
                    .into_iter()
                    .map(|(level, probability)| (level.to_owned(), probability))
                    .chain((0..3).map(|level| (level.to_string(), 0.0)))
                    .collect(),
                confidence: 0.5,
            })),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().clicks, ["New Message"]);
    assert!(
        !run.requests.iter().any(|request| request
            .questions
            .get("done")
            .is_some_and(|question| !text_of(question, "view").is_empty())),
        "no view is asked of a judgement that would not pass"
    );
}

#[tokio::test]
async fn a_missed_effect_asks_whether_the_press_was_intended() {
    // The checkbox never shows ticked: the predicted effect is missed, so
    // the next judgement asks whether the press did what it was meant to,
    // and a clear "no" undoes it.
    let run = run_with(
        App::with(|sim| {
            sim.pages = vec![EXTRAS];
            sim.quirks.insert(Quirk::Frozen);
        }),
        json!({"app": "Shop", "steps": ["add travel insurance"]}),
        |request| request.max_actions = 6,
        |id, question, _| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(pick(question, "Travel insurance", 0.9)),
            "intended" => Some(noul(0.1)),
            "done" => Some(noul(0.05)),
            _ => None,
        },
    )
    .await;
    assert!(asked(&run.requests, "intended") >= 1);
    assert!(loops(&run, 0).contains(&FlowLoop::Expectation));
    assert!(loops(&run, 0).contains(&FlowLoop::Undo));
}
