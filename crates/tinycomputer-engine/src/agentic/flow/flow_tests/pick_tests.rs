//! The `pick` and `extract` steps over lists of results.

use super::*;

fn flights() -> App {
    App::with(|sim| {
        sim.results = vec![
            ("IndiGo 6E-2135", "₹6,840", "6:45 PM"),
            ("Vistara UK-707", "₹7,210", "09:10"),
            ("Air India AI-825", "₹8,050", "05:30"),
        ];
    })
}

#[tokio::test]
async fn pick_ranks_a_measurable_criterion_exactly_and_opens_the_winner() {
    for (by, winner, airline) in [
        ("lowest price", "@s:select-1", "IndiGo"),
        ("earliest departure", "@s:select-3", "Air India"),
    ] {
        let run = run(
            flights(),
            json!({"app": "Mail", "steps": [
                {"pick": {"from": "the flight results", "by": by, "into": "flight"}},
                {"verify": "the picked flight's page is open"}
            ]}),
        )
        .await;
        assert_eq!(run.app.sim().picked, [winner], "{by}");
        assert!(run.result.vars["flight"].starts_with(airline), "{by}");
        assert!(
            run.result.steps[0].note.contains("ranked"),
            "{}",
            run.result.steps[0].note
        );
        assert!(
            !run.requests
                .iter()
                .any(|request| request.questions.contains_key("record")),
            "a measurable criterion needs no judgement"
        );
    }
}

#[tokio::test]
async fn pick_ranks_the_list_that_has_prices_not_the_longest_one() {
    let app = flights();
    app.sim().date_strip = 7;
    let run = run(
        app,
        json!({"app": "Mail", "steps": [
            {"pick": {"from": "the flight results", "by": "lowest price", "into": "flight"}}
        ]}),
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    assert_eq!(run.app.sim().picked, ["@s:select-1"]);
    assert!(run.result.vars["flight"].starts_with("IndiGo"));
    assert!(run.result.steps[0].note.contains("ranked"));
}

#[tokio::test]
async fn pick_asks_jev_when_the_criterion_needs_judgement() {
    let run = run_with(
        flights(),
        json!({"app": "Mail", "steps": [
            {"pick": {"from": "the flight results", "by": "the most comfortable airline"}}
        ]}),
        |_| {},
        |id, question, _| (id == "record").then(|| pick(question, "Vistara", 0.9)),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().picked, ["@s:select-2"]);
    assert!(run.result.steps[0].note.contains("judged"));
    assert!(
        !run.result.vars.contains_key("flight"),
        "no into, no variable"
    );

    let undecided = run_with(
        flights(),
        json!({"app": "Mail", "steps": [
            {"pick": {"from": "the flight results", "by": "the nicest"}}
        ]}),
        |_| {},
        |id, question, _| (id == "record").then(|| pick(question, "no such airline", 0.9)),
    )
    .await;
    assert_eq!(undecided.result.stop, FlowStopReason::StepFailed);
    assert!(undecided.result.steps[0].note.contains("clearly meets"));
}

#[tokio::test]
async fn pick_fails_where_no_list_is_showing() {
    let run = run(
        App::default(),
        json!({"app": "Mail", "steps": [
            {"pick": {"from": "the flight results", "by": "lowest price"}}
        ]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(
        run.result.steps[0]
            .note
            .contains("no list of the flight results")
    );
}

#[test]
fn pick_validates_its_fields_and_defines_its_variable() {
    let check = |flow: serde_json::Value| {
        super::validate::check(
            &serde_json::from_value(flow).unwrap(),
            &BTreeSet::new(),
            &BTreeSet::new(),
        )
        .errors
    };
    assert!(
        check(json!({"app": "Mail", "steps": [
            {"pick": {"from": "results", "by": "cheapest", "into": "flight"}},
            {"do": "book ${flight}"}
        ]}))
        .is_empty()
    );
    let errors = check(json!({"app": "Mail", "steps": [
        {"pick": {"from": "", "by": " ", "into": "not a name"}}
    ]}));
    assert_eq!(errors.len(), 3, "{errors:?}");
}

#[tokio::test]
async fn extract_stores_every_item_of_the_list() {
    let run = run(
        flights(),
        json!({"app": "Mail", "steps": [
            {"extract": {"what": "the flight results", "into": "flights"}}
        ]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    let rows: Vec<Vec<String>> = serde_json::from_str(&run.result.vars["flights"]).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][..2], ["IndiGo 6E-2135", "₹6,840"]);
    assert!(run.app.sim().picked.is_empty(), "extracting opens nothing");

    let nothing = run_with(
        App::default(),
        json!({"app": "Mail", "steps": [{"extract": {"what": "results", "into": "rows"}}]}),
        |_| {},
        |_, _, _| None,
    )
    .await;
    assert!(nothing.result.steps[0].note.contains("no list of results"));
}

#[tokio::test]
async fn a_judged_pick_chooses_within_the_list_it_names() {
    // Seven days above three flights: the judged pick is made among the
    // flights `from` names, not the longer strip of days.
    let app = flights();
    app.sim().date_strip = 7;
    let run = run_with(
        app,
        json!({"app": "Mail", "steps": [
            {"pick": {"from": "the flight results", "by": "the most comfortable airline"}}
        ]}),
        |_| {},
        |id, question, _| match id {
            "list" => Some(pick(question, "IndiGo", 0.9)),
            "record" => Some(pick(question, "Vistara", 0.9)),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().picked, ["@s:select-2"]);
}

#[tokio::test]
async fn extract_asks_which_list_is_meant_when_several_show() {
    // Seven days above three flights: the longest list is not the one
    // asked for, and Jev says which is.
    let app = flights();
    app.sim().date_strip = 7;
    let run = run_with(
        app,
        json!({"app": "Mail", "steps": [
            {"extract": {"what": "the flight results", "into": "flights"}}
        ]}),
        |_| {},
        |id, question, _| (id == "list").then(|| pick(question, "IndiGo", 0.9)),
    )
    .await;
    let rows: Vec<Vec<String>> = serde_json::from_str(&run.result.vars["flights"]).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][0], "IndiGo 6E-2135");

    // Unsure, it keeps the longest, as it did before it asked.
    let app = flights();
    app.sim().date_strip = 7;
    let unsure = run_with(
        app,
        json!({"app": "Mail", "steps": [
            {"extract": {"what": "the flight results", "into": "flights"}}
        ]}),
        |_| {},
        |id, question, _| (id == "list").then(|| pick(question, "IndiGo", 0.3)),
    )
    .await;
    let rows: Vec<Vec<String>> = serde_json::from_str(&unsure.result.vars["flights"]).unwrap();
    assert_eq!(rows.len(), 7);
}

#[tokio::test]
async fn a_picked_card_whose_control_is_covered_is_uncovered_and_opened() {
    // Live on Google Flights the cheapest card's "Select flight" link was
    // refused as covered and the pick failed; it now presses Escape and
    // tries the same card once more, as a `do` step's click does.
    let app = flights();
    app.sim().quirks.insert(Quirk::Drawer);
    let run = run(
        app,
        json!({"app": "Mail", "steps": [
            {"pick": {"from": "the flight results", "by": "lowest price"}}
        ]}),
    )
    .await;
    assert_eq!(run.result.steps[0].outcome, StepOutcome::Done);
    let sim = run.app.sim();
    assert_eq!(sim.presses, ["escape"]);
    assert_eq!(sim.picked, ["@s:select-1"]);
}

#[tokio::test]
async fn a_picked_card_that_cannot_be_opened_says_why() {
    let app = flights();
    app.sim().quirks.insert(Quirk::Unclickable);
    let run = run(
        app,
        json!({"app": "Mail", "steps": [
            {"pick": {"from": "the flight results", "by": "lowest price"}}
        ]}),
    )
    .await;
    let step = &run.result.steps[0];
    assert_eq!(step.outcome, StepOutcome::Failed);
    assert!(
        step.note.starts_with(
            "could not open the picked item (Element exists but is not visible.): IndiGo"
        ),
        "{}",
        step.note
    );
    assert!(
        run.app.sim().presses.is_empty(),
        "only a covered click is retried"
    );
}
