//! Budgets, invalid flows, and provider failures: a run stops cleanly.

use super::*;

#[tokio::test]
async fn the_implicit_launch_is_charged_to_the_action_budget() {
    // A run with no actions left must stop before touching the desktop at
    // all, and the launch it would otherwise perform for free must not be
    // missing from the reported action count on a run that does proceed.
    let starved = run_with(
        App::default(),
        mail_flow(),
        |request| request.max_actions = 0,
        |_, _, _| None,
    )
    .await;
    assert_eq!(starved.result.stop, FlowStopReason::ActionBudget);
    assert_eq!(starved.result.actions, 0);
    assert!(
        starved.app.sim().launched.is_empty(),
        "a starved run must never launch the application"
    );

    let launched = run(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
    )
    .await;
    assert_eq!(launched.app.sim().launched, ["Mail"]);
    assert!(
        launched.result.actions >= 1,
        "the implicit launch must count toward the reported actions"
    );
}

#[tokio::test]
async fn budgets_invalid_flows_and_provider_failures_stop_cleanly() {
    let actions = run_with(
        App::default(),
        mail_flow(),
        |request| request.max_actions = 1,
        |_, _, _| None,
    )
    .await;
    assert_eq!(actions.result.stop, FlowStopReason::ActionBudget);

    let calls = run_with(
        App::default(),
        mail_flow(),
        |request| request.max_model_calls = 1,
        |_, _, _| None,
    )
    .await;
    assert_eq!(calls.result.stop, FlowStopReason::ModelBudget);

    let unopened = run(
        App::quirky(Quirk::FailLaunch),
        json!({"app": "Mail", "steps": [{"open": "Nope"}]}),
    )
    .await;
    assert_eq!(unopened.result.stop, FlowStopReason::StepFailed);
    assert!(unopened.result.steps[0].note.contains("APP_NOT_FOUND"));

    let unreadable = run(
        App::quirky(Quirk::FailObserve),
        json!({"app": "Mail", "steps": ["anything"]}),
    )
    .await;
    assert!(
        unreadable.result.steps[0]
            .note
            .contains("could not be read")
    );

    let failing = Oracle {
        app: App::default(),
        hook: Box::new(|_, _, _| None),
        requests: Mutex::new(Vec::new()),
        fail: true,
    };
    let reply = run_flow_with(
        App::default(),
        &runtime(failing),
        RunFlowRequest {
            flow: serde_json::from_value(json!({"app": "Mail", "steps": ["x"]})).unwrap(),
            ..RunFlowRequest::default()
        },
    )
    .await;
    assert_eq!(reply.error.unwrap().code, "JEV_RATE_LIMITED");

    let invalid = run_flow_with(
        App::default(),
        &runtime(Oracle {
            app: App::default(),
            hook: Box::new(|_, _, _| None),
            requests: Mutex::new(Vec::new()),
            fail: false,
        }),
        RunFlowRequest::default(),
    )
    .await;
    assert_eq!(invalid.error.unwrap().code, "FLOW_INVALID");

    let fact_in_condition = run_flow_with(
        App::default(),
        &runtime(Oracle {
            app: App::default(),
            hook: Box::new(|_, _, _| None),
            requests: Mutex::new(Vec::new()),
            fail: false,
        }),
        RunFlowRequest {
            flow: serde_json::from_value(json!({"app": "Mail", "steps": [
                {"verify": "shows ${email}"}
            ]}))
            .unwrap(),
            vars: BTreeMap::from([("email".to_owned(), "sam@example.com".to_owned())]),
            facts: BTreeSet::from(["email".to_owned()]),
            ..RunFlowRequest::default()
        },
    )
    .await;
    let error = fact_in_condition.error.unwrap();
    assert_eq!(error.code, "FLOW_INVALID");
    assert!(
        error.message.contains("is a secret"),
        "a fact referenced outside an enter step never starts running: {error:?}"
    );
}
