//! Reflection: a step's result checked against what it was for, and
//! repaired once when it plainly missed.

use super::*;

/// A reflection hook: `reflects` and `strays` answer by whether the
/// simulator holds exactly one adult.
fn one_adult(id: &str, sim: &Sim) -> Option<Answer> {
    let right = sim.adults == Some(1);
    match id {
        "reflects" => Some(noul(if right { 0.9 } else { 0.1 })),
        "strays" => Some(noul(if right { 0.05 } else { 0.9 })),
        _ => None,
    }
}

#[tokio::test]
async fn a_choice_that_left_the_wrong_count_is_reflected_on_and_repaired() {
    let run = run_with(
        App::with(|sim| sim.adults = Some(1)),
        json!({"app": "browser", "steps": [
            {"choose": {"what": "the passengers box", "option": "1 Adult"}}
        ]}),
        |_| {},
        |id, question, sim| {
            one_adult(id, sim).or_else(|| match id {
                // Both steppers name "1 Adult"; the wrong one wins first.
                "target" if sim.adults == Some(1) => Some(pick(question, "Increase", 0.9)),
                "target" => Some(pick(question, "Decrease", 0.9)),
                "move" => Some(pick(question, "activate", 0.9)),
                "done" => Some(noul(if sim.adults == Some(1) { 0.95 } else { 0.05 })),
                _ => None,
            })
        },
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    assert_eq!(
        run.app.sim().adults,
        Some(1),
        "the repair undid the extra adult"
    );
    let clicks = run.app.sim().clicks.clone();
    assert!(clicks[0].starts_with("Increase"), "{clicks:?}");
    assert!(
        clicks.iter().any(|click| click.starts_with("Decrease")),
        "{clicks:?}"
    );
    let step = &run.result.steps[0];
    assert_eq!(step.outcome, StepOutcome::Done);
    assert!(step.loops.contains(&FlowLoop::Reflection));
}

#[tokio::test]
async fn a_choice_that_left_the_right_option_is_reflected_on_once() {
    let run = run_with(
        App::with(|sim| sim.extra_buttons = 9),
        json!({"app": "Mail", "steps": [{"choose": {"what": "the message list", "option": "Message 7"}}]}),
        |_| {},
        |_, _, _| None,
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(
        run.app.sim().clicks,
        ["Message 7"],
        "no repair pressed anything"
    );
    assert!(run.result.steps[0].loops.contains(&FlowLoop::Reflection));
    let reflections = run
        .requests
        .iter()
        .filter(|request| request.questions.contains_key("reflects"))
        .count();
    assert_eq!(reflections, 1);
}

#[tokio::test]
async fn a_repair_that_changes_nothing_fails_the_step_with_the_reflection() {
    let run = run_with(
        App::with(|sim| sim.adults = Some(1)),
        json!({"app": "browser", "steps": [
            {"choose": {"what": "the passengers box", "option": "1 Adult"}}
        ]}),
        |_| {},
        |id, question, sim| {
            one_adult(id, sim).or_else(|| match id {
                // Every press adds an adult: the repair only makes it worse.
                "target" => Some(pick(question, "Increase", 0.9)),
                "move" => Some(pick(question, "activate", 0.9)),
                "done" => Some(noul(0.05)),
                _ => None,
            })
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    let note = &run.result.steps[0].note;
    assert!(note.starts_with("reflection:"), "{note}");
    assert!(note.contains("1 Adult"), "{note}");
}

#[test]
fn a_selected_sibling_plainly_contradicts_the_option() {
    // Emirates puts each tab in its own list item.
    let tab = |name: &str, selected: bool| {
        let item = format!("listitem #{}", name.len());
        let mut tab = node(name, "tab", &["Click"], &["main", "tablist 2", &item], 1.0);
        if selected {
            tab.states = vec!["selected".to_owned()];
        }
        tab
    };
    let screen = |tabs: Vec<Candidate>| Screen {
        app: "browser".to_owned(),
        window: None,
        surface: "window".to_owned(),
        candidates: tabs,
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
    };
    assert_eq!(
        steps::left_unchosen(
            &screen(vec![tab("Return", true), tab("One way", false)]),
            "One way",
            false
        )
        .as_deref(),
        Some("tab \"One way\" is not selected; tab \"Return\" is")
    );
    assert!(
        steps::left_unchosen(
            &screen(vec![tab("Return", false), tab("One way", true)]),
            "One way",
            false
        )
        .is_none()
    );
    assert!(
        steps::left_unchosen(
            &screen(vec![tab("Return", false), tab("One way", false)]),
            "One way",
            false
        )
        .is_none(),
        "nothing selected settles nothing: Jev is asked"
    );
    assert!(
        steps::left_unchosen(&screen(vec![tab("Return", true)]), "Srinagar", false).is_none(),
        "an option no tab names is not settled here"
    );
}

#[tokio::test]
async fn a_tab_click_the_page_ignored_is_caught_even_when_jev_says_it_took() {
    let run = run_with(
        App::with(|sim| sim.trip = Some(("Return", 1))),
        json!({"app": "browser", "steps": [
            {"choose": {"what": "the trip type", "option": "One way"}}
        ]}),
        |_| {},
        |id, question, sim| match id {
            // A lenient Jev: it believes the choice took either way.
            "reflects" => Some(noul(0.9)),
            "strays" => Some(noul(0.05)),
            "target" => Some(pick(question, "One way", 0.9)),
            "move" => Some(pick(question, "activate", 0.9)),
            "done" => Some(noul(if sim.trip.is_some_and(|(tab, _)| tab == "One way") {
                0.95
            } else {
                0.05
            })),
            _ => None,
        },
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    assert_eq!(run.app.sim().trip.map(|(tab, _)| tab), Some("One way"));
    assert_eq!(
        run.app.sim().clicks,
        ["One way", "One way"],
        "pressed again by the repair"
    );
}

#[test]
fn an_option_still_offered_after_filtering_was_not_taken() {
    let option = node(
        "Dubai, United Arab Emirates Dubai International Airport DXB",
        "option",
        &["Click"],
        &["main", "listbox"],
        1.0,
    );
    let screen = Screen {
        app: "browser".to_owned(),
        window: None,
        surface: "window".to_owned(),
        candidates: vec![option.clone()],
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
    };
    let asked = "Dubai, United Arab Emirates Dubai International Airport DXB";
    assert!(
        steps::left_unchosen(&screen, asked, true)
            .is_some_and(|why| why.ends_with("is still offered, unselected"))
    );
    assert!(
        steps::left_unchosen(&screen, asked, false).is_none(),
        "a list the step did not filter keeps its options on screen"
    );
    let mut chosen = option;
    chosen.states = vec!["selected".to_owned()];
    let screen = Screen {
        candidates: vec![chosen],
        ..screen
    };
    assert!(steps::left_unchosen(&screen, asked, true).is_none());
}
