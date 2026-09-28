//! Whole flows against the simulated mail app: filling a compose window,
//! keeping facts out of Jev, gated and allowed destructive steps, and steps
//! already done.

use super::*;

#[tokio::test]
async fn a_mail_compose_flow_fills_every_field_and_stops_in_front_of_send() {
    let app = App::quirky(Quirk::BodyIgnoresSetValue);
    let run = run(app, mail_flow()).await;

    assert_eq!(run.result.stop, FlowStopReason::StoppedBeforeDestructive);
    assert_eq!(
        outcomes(&run.result),
        [
            ("1".to_owned(), StepOutcome::Done),
            ("2".to_owned(), StepOutcome::Done),
            ("3".to_owned(), StepOutcome::Done),
            ("4".to_owned(), StepOutcome::Done),
            ("5".to_owned(), StepOutcome::Gated),
        ]
    );
    let sim = run.app.sim();
    assert!(!sim.sent, "a gated flow must never press Send");
    assert_eq!(sim.fields["To"], "sam@example.com");
    assert_eq!(
        sim.fields["Body"],
        "Hi Sam,\n\nCould we move it to Friday?\n\nAlex"
    );
    assert_eq!(sim.presses, ["cmd+n"]);
    assert_eq!(
        run.result.pending.as_ref().unwrap().name.as_deref(),
        Some("Send")
    );

    let enter = &run.result.steps[2];
    assert!(enter.loops.contains(&FlowLoop::Slots));
    let body = enter
        .actions
        .iter()
        .find(|action| action.action == "fill message body")
        .unwrap();
    assert_eq!(
        body.note, "via paste",
        "an ignored set-value falls back to paste"
    );
    assert_eq!(
        enter.actions.first().unwrap().action,
        "fill recipient",
        "fields fill top to bottom"
    );
    assert!(
        run.result
            .learned
            .iter()
            .any(|hint| hint.key == "recipient" && hint.name.as_deref() == Some("To"))
    );
    assert!(run.result.metrics.calls > 0 && run.result.actions >= 5);
    assert_eq!(
        run.result.trace.len(),
        usize::try_from(run.result.metrics.calls).unwrap()
    );
    assert_eq!(run.result.trace[0].step, "2");
    assert!(
        choice_sizes(&run.requests)
            .iter()
            .all(|size| *size <= ask::CAP + 1)
    );
}

#[tokio::test]
async fn a_fact_is_typed_through_enter_but_never_reaches_a_jev_request() {
    let app = App::quirky(Quirk::BodyIgnoresSetValue);
    let run = run_with(
        app,
        mail_flow(),
        |request| {
            request.facts = BTreeSet::from(["to".to_owned()]);
            // A task's flow always runs with `include_values` off (screen
            // text is a separate, already-guarded leak path); this test is
            // about `${to}` substitution, the bug this change fixes.
            request.include_values = false;
        },
        |_, _, _| None,
    )
    .await;

    assert_eq!(run.result.stop, FlowStopReason::StoppedBeforeDestructive);
    let sim = run.app.sim();
    assert_eq!(
        sim.fields["To"], "sam@example.com",
        "the fact is still typed into the field"
    );

    let leaked = run.requests.iter().any(|request| {
        serde_json::to_string(request)
            .unwrap()
            .contains("sam@example.com")
    });
    assert!(!leaked, "a fact's value must never reach a Jev request");
    assert!(
        run.result
            .steps
            .iter()
            .all(|step| !step.text.contains("sam@example.com")
                && !step.note.contains("sam@example.com")),
        "a fact's value must not appear in a step report either"
    );
}

#[tokio::test]
async fn an_allowed_destructive_step_is_performed_and_verified() {
    let run = run_with(
        App::default(),
        mail_flow(),
        |request| request.allow_destructive = true,
        |_, _, _| None,
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert!(run.app.sim().sent);
    assert_eq!(run.result.steps[4].outcome, StepOutcome::Done);
}

#[tokio::test]
async fn a_step_already_accomplished_is_skipped_without_acting() {
    let app = App::with(|sim| sim.compose_open = true);
    let run = run_with(
        app,
        json!({"app": "Mail", "steps": ["show the compose window"]}),
        |_| {},
        |id, _, sim| (id == "done").then(|| noul(if sim.compose_open { 0.95 } else { 0.05 })),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.result.steps[0].outcome, StepOutcome::AlreadyDone);
    assert!(run.app.sim().presses.is_empty() && run.app.sim().clicks.is_empty());
}

#[tokio::test]
async fn something_new_is_never_taken_to_exist_before_acting() {
    // An open draft is someone's own; "start a new email" must make another.
    let app = App::with(|sim| sim.compose_open = true);
    let run = run_with(
        app,
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |_| {},
        |id, question, sim| match id {
            "move" if sim.presses.is_empty() => Some(pick(question, "finished", 0.9)),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.result.steps[0].outcome, StepOutcome::Done);
    assert_eq!(run.app.sim().presses, ["cmd+n"]);
    assert!(super::act::creates_new("Create a folder"));
    assert!(!super::act::creates_new("open the inbox"));
}

#[tokio::test]
async fn activating_a_control_learns_where_it_was() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |_| {},
        |id, question, _| (id == "move").then(|| pick(question, "activate", 0.9)),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().clicks, ["New Message"]);
    assert_eq!(run.result.learned[0].name.as_deref(), Some("New Message"));
}
