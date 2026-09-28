//! Flow validation: every problem reported by step, variables tracked in
//! execution order, and facts kept out of every model-facing position.

use super::*;

#[test]
fn validation_reports_every_problem_by_step() {
    let reply = validate_flow(&ValidateFlowRequest {
        flow: json!({
            "app": " ",
            "steps": [
                "",
                {"enter": {}},
                {"enter": {"x": "${missing}"}},
                {"read": {"what": "w", "into": "bad name"}},
                {"repeat_until": {"condition": "c", "steps": [], "max": 0}},
                {"if": {"condition": "c"}},
                {"choose": {"what": "", "option": "o"}},
                {"stop_before": "${also_missing}"},
                {"if": {"condition": "c", "then": [{"if": {"condition": "c", "then": [
                    {"if": {"condition": "c", "then": [{"if": {"condition": "c", "then": [
                        {"if": {"condition": "c", "then": ["deep"]}}]}}]}}]}}]}}
            ]
        }),
    });
    let validation: tinycomputer_bus::FlowValidation =
        serde_json::from_value(reply.data.unwrap()).unwrap();
    assert!(!validation.valid);
    let errors = validation.errors.join("\n");
    for expected in [
        "`app` must name",
        "step 1: the intent must not be empty",
        "step 2: `enter` needs at least one slot",
        "step 3: `${missing}` is not defined",
        "step 4: `into` must be a variable name",
        "step 5: `max` must be between 1 and 20",
        "step 5: `repeat_until` needs at least one step",
        "step 6: `if` needs a `then` or an `else` branch",
        "step 7: `what` must not be empty",
        "step 8: `${also_missing}` is not defined",
        "nests deeper than 4 levels",
    ] {
        assert!(
            errors.contains(expected),
            "missing {expected:?} in:\n{errors}"
        );
    }

    let malformed = validate::validate(
        &json!({"app": "Mail", "steps": [{"click": "x"}]}),
        &BTreeSet::new(),
        &BTreeSet::new(),
    );
    assert!(malformed.0.is_none());
    assert!(malformed.1.errors[0].contains("not well formed"));

    let empty = validate::check(&Flow::default(), &BTreeSet::new(), &BTreeSet::new());
    assert!(
        empty
            .errors
            .iter()
            .any(|error| error.contains("at least one step"))
    );

    let huge = Flow {
        app: "Mail".to_owned(),
        vars: BTreeMap::new(),
        steps: vec![tinycomputer_bus::FlowStep::Intent("x".to_owned()); 101],
    };
    assert!(
        validate::check(&huge, &BTreeSet::new(), &BTreeSet::new()).errors[0]
            .contains("at most 100")
    );

    let ok = validate::validate(&mail_flow(), &BTreeSet::new(), &BTreeSet::new());
    assert!(ok.0.is_some() && ok.1.valid && ok.1.steps == 5);
    let with_runtime_var = validate::check(
        &serde_json::from_value(json!({"app": "Mail", "steps": [{"open": "${app}"}]})).unwrap(),
        &BTreeSet::from(["app".to_owned()]),
        &BTreeSet::new(),
    );
    assert!(with_runtime_var.valid);
}

#[test]
fn validation_tracks_variables_along_execution_order() {
    let used_before_read = validate::check(
        &serde_json::from_value(json!({
            "app": "Mail",
            "steps": [
                {"verify": "shows ${name}"},
                {"read": {"what": "the name", "into": "name"}}
            ]
        }))
        .unwrap(),
        &BTreeSet::new(),
        &BTreeSet::new(),
    );
    assert!(
        used_before_read
            .errors
            .iter()
            .any(|error| error.contains("${name}") && error.contains("not defined")),
        "a read later in the flow must not define its variable for an earlier step"
    );

    let after_read = validate::check(
        &serde_json::from_value(json!({
            "app": "Mail",
            "steps": [
                {"read": {"what": "the name", "into": "name"}},
                {"verify": "shows ${name}"}
            ]
        }))
        .unwrap(),
        &BTreeSet::new(),
        &BTreeSet::new(),
    );
    assert!(after_read.valid);

    let only_in_untaken_branch = validate::check(
        &serde_json::from_value(json!({
            "app": "Mail",
            "steps": [
                {"if": {"condition": "c", "then": [
                    {"read": {"what": "the name", "into": "name"}}
                ]}},
                {"verify": "shows ${name}"}
            ]
        }))
        .unwrap(),
        &BTreeSet::new(),
        &BTreeSet::new(),
    );
    assert!(
        only_in_untaken_branch
            .errors
            .iter()
            .any(|error| error.contains("${name}") && error.contains("not defined")),
        "a read defined only inside one `if` branch must not survive it"
    );

    let only_in_repeat = validate::check(
        &serde_json::from_value(json!({
            "app": "Mail",
            "steps": [
                {"repeat_until": {"condition": "c", "steps": [
                    {"read": {"what": "the name", "into": "name"}}
                ]}},
                {"verify": "shows ${name}"}
            ]
        }))
        .unwrap(),
        &BTreeSet::new(),
        &BTreeSet::new(),
    );
    assert!(
        only_in_repeat
            .errors
            .iter()
            .any(|error| error.contains("${name}") && error.contains("not defined")),
        "a `repeat_until` body can run zero times, so its reads must not survive it"
    );
}

#[tokio::test]
async fn a_flow_definition_naming_a_caller_value_is_expanded_once() {
    let run = run_with(
        App::with(|sim| sim.compose_open = true),
        json!({
            "app": "Mail",
            "vars": {"subject_line": "${topic}"},
            "steps": [{"enter": {"subject": "${subject_line}", "body": "${note}"}}]
        }),
        |request| {
            request.vars = BTreeMap::from([
                ("topic".to_owned(), "Kashmir".to_owned()),
                ("note".to_owned(), "${topic}".to_owned()),
            ]);
        },
        |_, _, _| None,
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    let sim = run.app.sim();
    assert_eq!(
        sim.fields["Subject"], "Kashmir",
        "the definition names the caller's value"
    );
    assert_eq!(
        sim.fields["Body"], "${topic}",
        "a caller's value is text, never rescanned for references"
    );
}

#[tokio::test]
async fn a_flow_definition_naming_a_fact_is_typed_but_never_reaches_a_jev_request() {
    let run = run_with(
        App::with(|sim| sim.compose_open = true),
        json!({
            "app": "Mail",
            "vars": {"subject_line": "${topic}"},
            "steps": [{"enter": {"subject": "${subject_line}"}}]
        }),
        |request| {
            request.vars = BTreeMap::from([("topic".to_owned(), "Kashmir".to_owned())]);
            request.facts = BTreeSet::from(["topic".to_owned()]);
            request.include_values = false;
        },
        |_, _, _| None,
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    assert_eq!(
        run.app.sim().fields["Subject"],
        "Kashmir",
        "the definition carries the fact into the field"
    );
    let leaked = run
        .requests
        .iter()
        .any(|request| serde_json::to_string(request).unwrap().contains("Kashmir"));
    assert!(!leaked, "a fact's value must never reach a Jev request");
}

#[test]
fn validation_treats_a_flow_definition_naming_a_fact_as_a_fact() {
    let facts = BTreeSet::from(["email".to_owned()]);
    let flow: Flow = serde_json::from_value(json!({
        "app": "Mail",
        "vars": {"recipient": "${email}", "topic": "the budget"},
        "steps": [
            {"do": "write to ${recipient} about ${topic}"},
            {"enter": {"to": "${recipient}"}}
        ]
    }))
    .unwrap();
    let validation = validate::check(&flow, &facts, &facts);
    assert_eq!(
        validation.errors,
        vec![
            "step 1: `${recipient}` is a secret; only an enter step may type it — Jev only ever sees it as a name"
                .to_owned()
        ],
        "only the model-facing use of the fact-bearing definition is rejected"
    );
    assert_eq!(
        validate::carrying_facts(&flow.vars, &facts),
        BTreeSet::from(["email".to_owned(), "recipient".to_owned()]),
        "a definition naming a fact carries it; one that does not stays ordinary"
    );
}

#[test]
fn validation_rejects_a_fact_referenced_in_every_model_facing_position() {
    let flow: Flow = serde_json::from_value(json!({
        "app": "Mail",
        "steps": [
            {"open": "${email}"},
            {"browse": "${email}"},
            {"do": "tell Jev ${email}"},
            {"verify": "shows ${email}"},
            {"wait_for": "shows ${email}"},
            {"stop_before": "sending to ${email}"},
            {"choose": {"what": "${email}", "option": "ok"}},
            {"choose": {"what": "list", "option": "${email}"}},
            {"read": {"what": "the ${email} row", "into": "x"}},
            {"extract": {"what": "the ${email} row", "into": "y"}},
            {"pick": {"from": "${email}", "by": "lowest", "into": "z"}},
            {"pick": {"from": "results", "by": "${email}", "into": "w"}},
            {"repeat_until": {"condition": "shows ${email}", "steps": ["x"], "max": 1}},
            {"if": {"condition": "shows ${email}", "then": ["x"]}},
            {"enter": {"${email}": "hi"}}
        ]
    }))
    .unwrap();
    let facts = BTreeSet::from(["email".to_owned()]);
    let validation = validate::check(&flow, &facts, &facts);
    assert!(!validation.valid);
    let hits = validation
        .errors
        .iter()
        .filter(|error| error.contains("`${email}` is a secret"))
        .count();
    assert_eq!(
        hits,
        15,
        "every model-facing position should reject the fact:\n{}",
        validation.errors.join("\n")
    );
    for error in &validation.errors {
        if error.contains("`${email}` is a secret") {
            assert!(error.contains("only an enter step may type it"), "{error}");
        }
    }
}

#[test]
fn validation_allows_a_fact_only_as_an_enter_steps_typed_value() {
    let flow: Flow = serde_json::from_value(json!({
        "app": "Mail",
        "steps": [
            {"enter": {"email address": "${email}"}}
        ]
    }))
    .unwrap();
    let facts = BTreeSet::from(["email".to_owned()]);
    let validation = validate::check(&flow, &facts, &facts);
    assert!(validation.valid, "{:?}", validation.errors);
}

#[test]
fn validation_rejects_a_fact_reached_by_open_or_browse() {
    // The launched application or address becomes `screen.app` and a step
    // note in `history`, both of which reach Jev on a later step, so these
    // are rejected the same as any other model-facing position.
    let open = validate::check(
        &serde_json::from_value(json!({"app": "Mail", "steps": [{"open": "${app_name}"}]}))
            .unwrap(),
        &BTreeSet::from(["app_name".to_owned()]),
        &BTreeSet::from(["app_name".to_owned()]),
    );
    assert!(
        open.errors
            .iter()
            .any(|error| error.contains("`${app_name}` is a secret"))
    );

    let browse = validate::check(
        &serde_json::from_value(json!({"app": "browser", "steps": [{"browse": "${site}"}]}))
            .unwrap(),
        &BTreeSet::from(["site".to_owned()]),
        &BTreeSet::from(["site".to_owned()]),
    );
    assert!(
        browse
            .errors
            .iter()
            .any(|error| error.contains("`${site}` is a secret"))
    );
}

#[test]
fn validation_allows_a_non_fact_variable_in_model_facing_text() {
    let flow: Flow = serde_json::from_value(json!({
        "app": "Mail",
        "vars": {"topic": "the budget"},
        "steps": [
            {"do": "start an email about ${topic}"},
            {"verify": "mentions ${topic}"}
        ]
    }))
    .unwrap();
    let validation = validate::check(&flow, &BTreeSet::new(), &BTreeSet::new());
    assert!(validation.valid, "{:?}", validation.errors);
}

#[test]
fn validation_rejects_a_condition_that_names_a_picked_item() {
    // A pick stores the whole card's text, and the opened item no longer
    // shows it all: a condition built on it fails a pick that worked.
    let flow: Flow = serde_json::from_value(json!({
        "app": "browser",
        "steps": [
            {"pick": {"from": "the flight results", "by": "lowest price", "into": "cheapest_flight"}},
            {"verify": "${cheapest_flight} shows a price and airline"},
            {"wait_for": "the fare for ${cheapest_flight} is listed"},
            {"repeat_until": {"condition": "${cheapest_flight} is gone", "max": 2, "steps": ["scroll down"]}},
            {"if": {"condition": "a banner is showing", "then": [
                {"verify": "${cheapest_flight} is selected"}
            ]}},
            {"do": "open the details of ${cheapest_flight}"}
        ]
    }))
    .unwrap();
    let validation = validate::check(&flow, &BTreeSet::new(), &BTreeSet::new());
    let picked = validation
        .errors
        .iter()
        .filter(|error| error.contains("`${cheapest_flight}` holds a picked item"))
        .collect::<Vec<_>>();
    assert_eq!(picked.len(), 4, "{:?}", validation.errors);
    for path in ["step 2:", "step 3:", "step 4:", "step 5.1:"] {
        assert!(
            picked.iter().any(|error| error.starts_with(path)),
            "{path} {picked:?}"
        );
    }
}

#[test]
fn a_variable_named_without_its_braces_is_rejected() {
    let flow: Flow = serde_json::from_value(json!({
        "app": "browser",
        "steps": [
            {"pick": {"from": "the flights", "by": "lowest price", "into": "cheapest_flight"}},
            {"do": "book cheapest_flight"},
            {"do": "book ${cheapest_flight}"},
            {"pick": {"from": "the fares", "by": "lowest price", "into": "fare"}},
            {"verify": "the fare is shown"}
        ]
    }))
    .unwrap();
    let validation = validate::check(&flow, &BTreeSet::new(), &BTreeSet::new());
    assert_eq!(
        validation.errors,
        [
            "step 2: `cheapest_flight` names a variable; write it as `${cheapest_flight}` so its value is shown"
        ],
        "only the bare identifier is an error; a plain word naming a variable is not"
    );
}
