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

fn toast() -> App {
    App::with(|sim| {
        sim.quirks.insert(Quirk::PromoToast);
    })
}

#[tokio::test]
async fn a_promo_toast_is_cleared_before_the_step() {
    let run = run_with(
        toast(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |_| {},
        |id, question, _| match id {
            "focus" => Some(pick(question, "Unlimited date changes", 0.9)),
            "move" => Some(pick(question, "activate", 0.9)),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().clicks, ["Close", "New Message"]);
    assert!(loops(&run, 0).contains(&FlowLoop::Attention));
    let Question::Choice(focus) = run
        .requests
        .iter()
        .find_map(|request| request.questions.get("focus"))
        .unwrap()
    else {
        panic!("focus is a choice")
    };
    assert!(focus.criteria.contains_key("step"));
    assert_eq!(focus.criteria.len(), 3, "the step, the one toast, and none");
}

#[tokio::test]
async fn a_screen_where_nothing_is_in_the_way_asks_nothing() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |_| {},
        |id, question, _| (id == "move").then(|| pick(question, "activate", 0.9)),
    )
    .await;
    assert_eq!(asked(&run.requests, "focus"), 0);
    assert!(!loops(&run, 0).contains(&FlowLoop::Attention));
}

#[tokio::test]
async fn a_toast_is_cleared_before_a_choose_grounds() {
    let run = run_with(
        App::with(|sim| {
            sim.quirks.insert(Quirk::PromoToast);
            sim.trip = Some(("Return", 0));
        }),
        json!({"app": "Mail", "steps": [{"choose": {"what": "the trip type tabs", "option": "One way"}}]}),
        |_| {},
        |id, question, _| (id == "focus").then(|| pick(question, "Unlimited date changes", 0.9)),
    )
    .await;
    assert_eq!(
        run.app.sim().clicks.first().map(String::as_str),
        Some("Close")
    );
    assert!(run.app.sim().clicks.contains(&"One way".to_owned()));
    assert!(loops(&run, 0).contains(&FlowLoop::Attention));
}

#[tokio::test]
async fn a_toast_jev_says_is_not_in_the_way_is_left() {
    let run = run_with(
        toast(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |_| {},
        |id, question, _| match id {
            "focus" => Some(pick(question, "step", 0.9)),
            "move" => Some(pick(question, "activate", 0.9)),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert!(!run.app.sim().clicks.contains(&"Close".to_owned()));
    assert!(asked(&run.requests, "focus") >= 1);
}

#[tokio::test]
async fn a_failed_choose_puts_back_the_text_it_typed_by_mistake() {
    // Live on Emirates: no gender field, so "Female" was typed wherever the
    // focus was — the last name filled a step earlier — and the step failed
    // leaving "RainaFemale" behind.
    let flow = json!({"app": "Mail", "steps": [
        {"enter": {"recipient": "sam@example.com"}},
        {"choose": {"what": "the gender field", "option": "Female"}}
    ]});
    let hook = |id: &str, question: &Question, _: &Sim| match id {
        // Nothing opens a gender list: there is none.
        "move" => Some(pick(question, "stuck", 0.9)),
        _ => None,
    };
    let app = || {
        App::with(|sim| {
            sim.compose_open = true;
            sim.quirks.insert(Quirk::FocusStays);
        })
    };
    let deep = run_with(app(), flow.clone(), |_| {}, hook).await;
    assert_eq!(deep.result.stop, FlowStopReason::StepFailed);
    assert_eq!(deep.app.sim().fields["To"], "sam@example.com");
    assert!(loops(&deep, 1).contains(&FlowLoop::Checkpoint));
    let off = run_with(
        app(),
        flow,
        |request| request.deliberation = Deliberation::Off,
        hook,
    )
    .await;
    assert_eq!(
        off.app.sim().fields["To"],
        "sam@example.comFemale",
        "without deliberation the stray text stays"
    );
}

#[tokio::test]
async fn escape_at_a_covering_is_pressed_once_per_step() {
    // Live on Emirates, an open calendar covered the Class button and Escape
    // did not close it: pressing it again every loop only wastes turns.
    let run = run_with(
        App::with(|sim| {
            sim.quirks.insert(Quirk::Covered);
        }),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| request.max_actions = 6,
        |id, question, _| match id {
            "focus" => Some(pick(question, "something open over the page", 0.9)),
            "move" => Some(pick(question, "stuck", 0.9)),
            _ => None,
        },
    )
    .await;
    let escapes = run
        .app
        .sim()
        .presses
        .iter()
        .filter(|press| *press == "escape")
        .count();
    assert_eq!(escapes, 1);
    assert!(loops(&run, 0).contains(&FlowLoop::Attention));
}

#[tokio::test]
async fn a_clear_decision_is_asked_three_ways_and_a_split_one_every_way() {
    let with = |votes: u32, deliberation: Deliberation| {
        run_with(
            App::default(),
            mail_flow(),
            move |request| {
                request.votes = votes;
                request.deliberation = deliberation;
            },
            |_, _, _| None,
        )
    };
    let three = with(3, Deliberation::Deep).await;
    let seven = with(7, Deliberation::Deep).await;
    let legacy = with(7, Deliberation::Off).await;
    assert_eq!(seven.result.stop, three.result.stop);
    assert_eq!(
        seven.requests.len(),
        three.requests.len(),
        "framings that agree are not asked again: seven votes cost what three do"
    );
    assert!(
        legacy.requests.len() > seven.requests.len(),
        "the legacy path asks every vote"
    );

    // A slot choice the first framings split on is asked all seven ways.
    let biased = biased_mail(7).await;
    let slot_asks = asked_prefix(&biased.requests, "slot_");
    assert_eq!(slot_asks % 7, 0, "{slot_asks}");
    assert_eq!(
        biased.app.sim().fields["Subject"],
        "Moving Thursday's sync",
        "the split is settled by every vote"
    );
}

#[test]
fn a_ballot_is_settled_only_when_every_framing_agrees() {
    use std::collections::BTreeMap;
    use tinyinference_decisions::{ChoiceAnswer, NoulAnswer};

    let noul = |values: &[f64]| {
        values
            .iter()
            .map(|noul| Answer::Noul(NoulAnswer { noul: *noul }))
            .collect::<Vec<_>>()
    };
    let choice = |picks: &[&str]| {
        picks
            .iter()
            .map(|pick| {
                Answer::Choice(ChoiceAnswer {
                    choice: (*pick).to_owned(),
                    probabilities: BTreeMap::from([((*pick).to_owned(), 0.8)]),
                    confidence: 0.8,
                })
            })
            .collect::<Vec<_>>()
    };
    let ballots = |pairs: Vec<(&str, Vec<Answer>)>| {
        pairs
            .into_iter()
            .map(|(id, answers)| (id.to_owned(), answers))
            .collect::<BTreeMap<_, _>>()
    };
    let settled = |pairs| vote::settled(&ballots(pairs), PAGE_KIND);
    assert!(settled(vec![("target", choice(&["3", "3", "3"]))]));
    assert!(!settled(vec![("target", choice(&["3", "3", "4"]))]));
    assert!(settled(vec![("done", noul(&[0.8, 0.9, 0.75]))]));
    assert!(!settled(vec![("done", noul(&[0.55, 0.9, 0.8]))]), "too spread");
    assert!(!settled(vec![("done", noul(&[0.45, 0.55, 0.5]))]), "both sides");
    assert!(!settled(vec![("done", noul(&[0.9]))]), "one answer shows nothing");
    assert!(
        settled(vec![
            ("done", noul(&[0.1, 0.2, 0.15])),
            (PAGE_KIND, choice(&["results", "form", "results"]))
        ]),
        "the page kind rides along and never holds a decision back"
    );
}
