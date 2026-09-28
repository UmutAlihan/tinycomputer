//! The step kinds other than `do`, `choose`, `enter`, and `pick`: `read`,
//! `if`, `repeat_until`, `verify`, `wait_for`, `open`, and `browse`.

use super::*;

#[tokio::test]
async fn a_read_target_beyond_the_source_cap_is_still_found_by_paging() {
    // 72 candidates ("New Message", "Archive", and 70 message rows) exceed
    // `ask::MAX_READ_SOURCES` (60); a target past that cutoff must still be
    // reachable a page at a time rather than permanently dropped.
    let run = run_with(
        App::with(|sim| sim.extra_buttons = 70),
        json!({"app": "Mail", "steps": [
            {"read": {"what": "the row for message 65", "into": "row"}}
        ]}),
        |_| {},
        |id, question, _| (id == "source").then(|| pick(question, "Message 65", 0.9)),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.result.vars["row"], "Message 65");
}

#[tokio::test]
async fn a_read_can_take_an_elements_name_rather_than_its_value() {
    // The Subject field is named "Subject" and holds what was typed: asked
    // for the field's name, the read gets the name, not the value.
    let answers = |part: &'static str| {
        move |id: &str, question: &Question, _: &Sim| {
            (id == "source").then(|| pick(question, &format!("\"part\":\"{part}\""), 0.9))
        }
    };
    for (part, read) in [("name", "Subject"), ("value", "Moving Thursday")] {
        let run = run_with(
            App::default(),
            json!({"app": "Mail", "steps": [
                "start a new email message",
                {"enter": {"subject": "Moving Thursday"}},
                {"read": {"what": "the subject field", "into": "got"}}
            ]}),
            |_| {},
            answers(part),
        )
        .await;
        assert_eq!(run.result.stop, FlowStopReason::Completed, "{part}");
        assert_eq!(run.result.vars["got"], read, "{part}");
    }
}

#[tokio::test]
async fn later_steps_remember_what_earlier_steps_saved() {
    // A flow that walks a list must know which items it has already done:
    // every state after a read recalls the value, and an extract's rows as
    // their count.
    let app = flights();
    let run = run_with(
        app,
        json!({"app": "Mail", "steps": [
            {"read": {"what": "the window heading", "into": "heading"}},
            {"extract": {"what": "the flight results", "into": "flights"}},
            {"read": {"what": "the first Select button", "into": "again"}}
        ]}),
        |_| {},
        |id, question, _| (id == "source").then(|| pick(question, "Select", 0.9)),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    let first = run.requests.first().unwrap();
    assert!(
        first.state.get("already_collected").is_none(),
        "nothing is recalled before anything is saved"
    );
    let last = run.requests.last().unwrap();
    let recalled = &last.state["already_collected"]["untrusted_accessibility_data"];
    assert_eq!(recalled["heading"], run.result.vars["heading"]);
    assert!(
        recalled["flights"]
            .as_str()
            .unwrap()
            .starts_with("3 items; the first: IndiGo 6E-2135"),
        "{recalled}"
    );
}

#[tokio::test]
async fn control_steps_branch_repeat_read_and_wait() {
    let run = run_with(
        App::default(),
        json!({
            "app": "Mail",
            "steps": [
                {"if": {"condition": "a compose window is open",
                        "then": [{"verify": "never taken"}],
                        "else": ["start a new email message"]}},
                {"repeat_until": {"condition": "a compose window is open",
                                  "steps": ["start a new email message"], "max": 2}},
                {"wait_for": "a compose window is open"},
                {"read": {"what": "the window heading", "into": "heading"}},
                {"enter": {"subject": "About ${heading}"}}
            ]
        }),
        |_| {},
        |id, question, _| (id == "source").then(|| pick(question, "New Message heading", 0.9)),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.result.vars["heading"], "New Message heading");
    assert_eq!(run.app.sim().fields["Subject"], "About New Message heading");
    let paths = run
        .result
        .steps
        .iter()
        .map(|step| step.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["1", "1.1", "2", "3", "4", "5"]);
}

#[tokio::test]
async fn a_repeat_that_never_holds_and_a_failing_verify_fail_the_flow() {
    let repeat = run(
        App::quirky(Quirk::Frozen),
        json!({"app": "Mail", "steps": [
            {"repeat_until": {"condition": "a compose window is open",
                              "steps": [{"wait_for": "nothing"}], "max": 1}}
        ]}),
    )
    .await;
    assert_eq!(repeat.result.stop, FlowStopReason::StepFailed);

    let verify = run(
        App::default(),
        json!({"app": "Mail", "steps": [{"verify": "a compose window is open"}]}),
    )
    .await;
    assert_eq!(verify.result.stop, FlowStopReason::StepFailed);
    assert!(verify.result.steps[0].note.contains("does not hold"));

    let branch_then = run(
        App::with(|sim| sim.compose_open = true),
        json!({"app": "Mail", "steps": [
            {"if": {"condition": "a compose window is open", "then": [{"verify": "a compose window is open"}]}},
            {"repeat_until": {"condition": "a compose window is open", "steps": ["x"]}}
        ]}),
    )
    .await;
    assert_eq!(branch_then.result.stop, FlowStopReason::Completed);
}

#[tokio::test]
async fn a_repeat_conditions_trace_is_attributed_to_the_repeat_step_not_its_last_child() {
    // Round 0 runs its child ("start a new email message", path "1.r1.1"),
    // which opens the compose window. Round 1's condition check must then be
    // traced to "1", the repeat_until step itself, not left tagged with the
    // path of the child that last ran.
    let run = run(
        App::default(),
        json!({"app": "Mail", "steps": [
            {"repeat_until": {"condition": "a compose window is open",
                              "steps": ["start a new email message"], "max": 2}}
        ]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(
        run.result
            .trace
            .last()
            .map(|exchange| exchange.step.as_str()),
        Some("1"),
        "the condition check that ended the loop belongs to the repeat_until step"
    );
}

#[tokio::test]
async fn fields_in_a_truncated_subtree_are_found_by_exploring_it() {
    let run = run(
        App::with(|sim| {
            sim.compose_open = true;
            sim.quirks.insert(Quirk::HiddenEditor);
        }),
        json!({"app": "Mail", "steps": [{"enter": {"subject": "Found it"}}]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().fields["Subject"], "Found it");
}

#[tokio::test]
async fn an_app_that_never_shows_a_window_is_reported_as_opened_but_unreadable() {
    let run = run(
        App::quirky(Quirk::FailObserve),
        json!({"app": "Mail", "steps": [{"open": "Mail"}]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert!(run.result.steps[0].note.contains("no readable window yet"));
}

#[tokio::test]
async fn browse_opens_the_address_and_moves_the_flow_onto_the_page() {
    let run = run(
        App::default(),
        json!({
            "app": "Mail",
            "vars": {"to": "Srinagar"},
            "steps": [{"browse": "https://flights.test/to/${to}"}]
        }),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().launched, ["Mail", "browser"]);
    assert_eq!(
        run.app.sim().navigated,
        ["https://flights.test/to/Srinagar"]
    );
    assert_eq!(
        run.result.steps[0].note,
        "https://flights.test/to/Srinagar is open (Flights)"
    );
}

#[tokio::test]
async fn browse_fails_the_flow_where_there_is_no_browser_or_no_page() {
    for (quirk, code) in [
        (Quirk::FailLaunch, "APP_NOT_FOUND"),
        (Quirk::NoAddresses, "ACTION_NOT_SUPPORTED"),
    ] {
        let run = run(
            App::quirky(quirk),
            json!({"app": "browser", "steps": [{"browse": "https://flights.test"}]}),
        )
        .await;
        assert_eq!(run.result.stop, FlowStopReason::StepFailed, "{quirk:?}");
        assert!(
            run.result.steps[0].note.contains(code),
            "{}",
            run.result.steps[0].note
        );
    }
    let unreadable = run(
        App::quirky(Quirk::FailObserve),
        json!({"app": "browser", "steps": [{"browse": "https://flights.test"}]}),
    )
    .await;
    assert!(
        unreadable.result.steps[0]
            .note
            .contains("no readable page yet")
    );
}
