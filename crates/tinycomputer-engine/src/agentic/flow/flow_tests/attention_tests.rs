//! Deliberation's attention: distractions cleared before a step, text
//! typed by mistake put back, and Escape at a covering pressed once.

use super::*;

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
async fn an_enter_with_no_form_on_screen_stops_after_the_first_missing_picker() {
    // Nothing on screen takes text and nothing reveals a field: the step is
    // on the wrong page, so one picker is tried, not one per detail.
    let blind = |slots: Value| {
        run_with(
            App::default(),
            json!({"app": "Mail", "steps": [{"enter": slots}]}),
            |request| request.max_actions = 20,
            |id, question, _| {
                (id.starts_with("slot_") || id == "target" || id == "move" || id == "option")
                    .then(|| pick(question, "none", 0.9))
            },
        )
    };
    let one = blind(json!({"shoe size": "11"})).await;
    let three = blind(json!({"shoe size": "11", "hat size": "7", "glove size": "8"})).await;
    assert_eq!(three.result.stop, FlowStopReason::StepFailed);
    assert!(
        three.result.steps[0]
            .note
            .contains("no field that takes text was found for: glove size, hat size, shoe size"),
        "{}",
        three.result.steps[0].note
    );
    let pickers = |run: &Run| asked(&run.requests, "target") + asked(&run.requests, "option");
    assert_eq!(
        pickers(&three),
        pickers(&one),
        "the second and third details are not looked for one by one"
    );
}
