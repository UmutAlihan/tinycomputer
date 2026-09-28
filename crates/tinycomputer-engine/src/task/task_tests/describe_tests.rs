//! Tests for `Describe`: every member documented and catalogued, each
//! schema naming its request's fields, and examples that really decode.

use super::*;

#[tokio::test]
async fn describe_documents_every_member_and_its_examples_really_work() {
    let (tasks, _) = controller(Vec::new());
    let described = capabilities(Vec::new(), true, &tasks);
    let names = described
        .members
        .iter()
        .map(|member| member.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, tinycomputer_bus::agent::names::METHODS);
    let confidential = described
        .members
        .iter()
        .filter(|member| member.confidential)
        .map(|member| member.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(confidential, tinycomputer_bus::agent::names::CONFIDENTIAL);
    assert!(described.step_kinds.iter().any(|kind| kind == "browse"));
    assert!(!described.planner_configured);
    assert!(!described.rescue_configured);
    assert!(!described.output_configured);

    let flight = &described.examples[0];
    assert_eq!(flight.member, "StartTask");
    let request: StartTaskRequest = serde_json::from_value(flight.request.clone()).unwrap();
    let (tasks, _) = controller(Vec::new());
    let view = tasks.start(&request).data.unwrap();
    let TaskStatus::NeedsInput { fields } = view.status else {
        panic!("the example leaves one fact for the caller to supply");
    };
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].name, "phone");
    assert_eq!(fields[0].kind, InputKind::Phone);
    for example in &described.examples[1..] {
        assert!(
            tinycomputer_bus::names::METHODS.contains(&example.member.as_str()),
            "{} is not a served member",
            example.member
        );
    }
}

#[tokio::test]
async fn describe_catalogues_every_served_member() {
    let (tasks, _) = controller(Vec::new());
    let described = capabilities(Vec::new(), true, &tasks);
    let names = described
        .catalogue
        .iter()
        .map(|member| member.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, tinycomputer_bus::names::METHODS);
}

/// The property names a hand-written object schema documents.
fn documented(schema: &serde_json::Value) -> Vec<String> {
    let mut names = schema["properties"]
        .as_object()
        .expect("an object schema")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// The field names a value serializes with, every optional one set.
fn fields(value: &serde_json::Value) -> Vec<String> {
    let mut names = value
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[tokio::test]
async fn describe_schemas_name_every_request_field() {
    let (tasks, _) = controller(Vec::new());
    let described = capabilities(Vec::new(), true, &tasks);
    let schema = |name: &str| {
        described
            .members
            .iter()
            .find(|member| member.name == name)
            .map(|member| member.input.clone())
            .expect("the member is documented")
    };

    let start = StartTaskRequest {
        output: Some(tinycomputer_bus::agent::TaskOutput::default()),
        ..StartTaskRequest::default()
    };
    let start = serde_json::to_value(start).unwrap();
    let start_schema = schema("StartTask");
    assert_eq!(documented(&start_schema), fields(&start));
    assert_eq!(
        documented(&start_schema["properties"]["constraints"]),
        fields(&start["constraints"])
    );
    let budget = tinycomputer_bus::agent::TaskBudget {
        max_actions: Some(1),
        max_model_calls: Some(1),
        votes: Some(1),
        strategy: Some(tinycomputer_bus::FlowStrategy::default()),
        deliberation: Some(tinycomputer_bus::Deliberation::default()),
        max_elapsed_ms: Some(1),
        max_rescues: Some(1),
    };
    assert_eq!(
        documented(&start_schema["properties"]["budget"]),
        fields(&serde_json::to_value(budget).unwrap())
    );
    assert_eq!(
        documented(&start_schema["properties"]["output"]),
        vec!["instructions".to_owned(), "schema".to_owned()]
    );

    let plan = serde_json::to_value(tinycomputer_bus::agent::PlanTaskRequest::default()).unwrap();
    assert_eq!(documented(&schema("PlanTask")), fields(&plan));

    let id = tinycomputer_bus::agent::TaskId::new("t-1");
    let others = [
        (
            "AwaitTask",
            serde_json::to_value(tinycomputer_bus::agent::AwaitTaskRequest {
                id: id.clone(),
                timeout_ms: 1,
            }),
        ),
        (
            "ContinueTask",
            serde_json::to_value(tinycomputer_bus::agent::ContinueTaskRequest {
                id: id.clone(),
                approve: Some(true),
                answer: Some(String::new()),
                ..tinycomputer_bus::agent::ContinueTaskRequest::default()
            }),
        ),
        (
            "CancelTask",
            serde_json::to_value(tinycomputer_bus::agent::TaskRef { id: id.clone() }),
        ),
        (
            "TaskReport",
            serde_json::to_value(tinycomputer_bus::agent::TaskReportRequest::new(id)),
        ),
    ];
    for (member, request) in others {
        assert_eq!(
            documented(&schema(member)),
            fields(&request.unwrap()),
            "{member}"
        );
    }
    // Every member with a request object is compared above; the rest take
    // no argument.
    for member in &described.members {
        if member.input["type"] != "object" {
            assert_eq!(member.input["type"], "null", "{}", member.name);
        }
    }
}

#[tokio::test]
async fn the_task_report_schema_requires_the_trace_flag() {
    // A schema-driven caller sends only what is required; a body of only
    // `{"id"}` would be refused client-side as a stream handle.
    let (tasks, _) = controller(Vec::new());
    let described = capabilities(Vec::new(), true, &tasks);
    let report = described
        .members
        .iter()
        .find(|member| member.name == "TaskReport")
        .unwrap();
    assert_eq!(report.input["required"], json!(["id", "trace"]));
}

#[tokio::test]
async fn every_describe_example_decodes_as_its_members_request() {
    use tinycomputer_bus::browser::{
        Action, NavigateRequest, ReadOutputRequest, SessionOptions, SessionRequest, names,
    };
    let (tasks, _) = controller(Vec::new());
    let described = capabilities(Vec::new(), true, &tasks);
    let mut seen = 0;
    for example in &described.examples {
        let request = example.request.clone();
        let decoded = match example.member.as_str() {
            names::methods::OPEN_SESSION => {
                serde_json::from_value::<SessionOptions>(request).is_ok()
            }
            names::methods::NAVIGATE => {
                serde_json::from_value::<SessionRequest<NavigateRequest>>(request).is_ok()
            }
            names::methods::PERFORM => {
                serde_json::from_value::<SessionRequest<Action>>(request).is_ok()
            }
            names::methods::READ_OUTPUT => {
                serde_json::from_value::<ReadOutputRequest>(request).is_ok()
            }
            "StartTask" => serde_json::from_value::<StartTaskRequest>(request).is_ok(),
            "AwaitTask" => serde_json::from_value::<AwaitTaskRequest>(request).is_ok(),
            "ContinueTask" => {
                serde_json::from_value::<tinycomputer_bus::agent::ContinueTaskRequest>(request)
                    .is_ok()
            }
            other => panic!("the {other} example has no decoder here; add one"),
        };
        assert!(decoded, "the {} example does not decode", example.title);
        seen += 1;
    }
    assert_eq!(seen, described.examples.len());
}
