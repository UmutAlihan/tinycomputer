//! Deliberation's recovery: oscillations banned, wrong presses undone and
//! verified, runners-up tried, and irreversible presses held to a deep
//! accept.

use super::*;

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

#[tokio::test]
async fn a_duel_champions_own_share_still_needs_a_deep_accept() {
    // Two lookalike "Select" buttons force `stop_before`'s target through a
    // duel: a tied initial Choice escalates to widening, then a duel names
    // one a champion by only DUEL_WIN's margin — a real but middling
    // belief, nowhere near IRREVERSIBLE_FLOOR. Its confidence must be that
    // measured share, not a blanket 1.0 that would skip `vouch` outright.
    let flow = json!({"app": "Mail", "steps": [{"stop_before": "selecting the 09:00 flight"}]});
    let answer = |id: &str, question: &Question| match id {
        "target" => Some(weighted(
            question,
            &[("listitem #1", 0.45), ("listitem #2", 0.45)],
        )),
        // Every framing of the duel gives "listitem #2" a share of about
        // 0.71 — over DUEL_WIN (0.6), so it is crowned champion, but well
        // under IRREVERSIBLE_FLOOR (0.85).
        _ if id.starts_with("duel_") => Some(pick(question, "listitem #2", 0.55)),
        _ => None,
    };
    let unsure = run_with(
        lookalikes(),
        flow.clone(),
        |request| request.allow_destructive = true,
        move |id, question, _| match id {
            "is_0" => Some(noul(0.5)),
            _ => answer(id, question),
        },
    )
    .await;
    assert_eq!(
        unsure.result.stop,
        FlowStopReason::StepFailed,
        "a champion's own ~0.71 share must not clear the irreversible floor by itself"
    );
    assert!(unsure.result.steps[0].note.contains("uncertain evidence"));
    for used in [FlowLoop::Evidence, FlowLoop::Escalation, FlowLoop::Duel] {
        assert!(loops(&unsure, 0).contains(&used), "{used:?}");
    }

    let sure = run_with(
        lookalikes(),
        flow,
        |request| request.allow_destructive = true,
        move |id, question, _| match id {
            "is_0" => Some(noul(0.97)),
            "holds" => Some(noul(0.95)),
            _ => answer(id, question),
        },
    )
    .await;
    assert_eq!(
        sure.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        sure.result.steps
    );
    assert_eq!(sure.app.sim().picked, ["@s:select-2"]);
}
