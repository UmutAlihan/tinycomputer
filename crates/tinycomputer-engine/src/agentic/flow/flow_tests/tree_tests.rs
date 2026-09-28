//! Deliberation's tree grounding and views: a close second region kept, a
//! lost target caught by the wider Choice, and judgements read over other
//! renderings of the screen.

use super::*;

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
