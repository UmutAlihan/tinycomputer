//! Tests for the task members over a real bus, and the runner's workspaces.

use super::service;
use crate::tinybus_module::setup;
use serde_json::json;
use tinybus::broker::Broker;
use tinybus::transport::memory::MemoryBus;
use tinybus::{Connection, Interface};
use tinycomputer_bus::names;

#[tokio::test]
async fn the_ordinary_task_members_answer_over_a_real_bus() -> tinybus::Result<()> {
    use tinycomputer_bus::agent::{AgentResponse, Capabilities, TaskView};

    let bus = MemoryBus::new();
    Broker::new().spawn(bus.clone());
    let service = Connection::connect(bus.connect().await?).await?;
    setup(service.clone(), json!({})).await?;
    let client = Connection::connect(bus.connect().await?).await?;
    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;

    let described: Capabilities = proxy.call(names::methods::DESCRIBE, json!([])).await?;
    assert!(!described.jev_configured);
    assert_eq!(described.members.len(), 8);
    assert!(described.surfaces.iter().any(|surface| surface.kind
        == tinycomputer_bus::agent::SurfaceKind::Browser
        && surface.available));

    let listed: AgentResponse<Vec<TaskView>> =
        proxy.call(names::methods::LIST_TASKS, json!([])).await?;
    assert_eq!(listed.data.unwrap().len(), 0);

    let planned: AgentResponse<serde_json::Value> = proxy
        .call(
            names::methods::PLAN_TASK,
            json!([{"task": "book a flight"}]),
        )
        .await?;
    assert_eq!(planned.error.unwrap().code, "PLANNER_NOT_CONFIGURED");

    for member in [names::methods::AWAIT_TASK, names::methods::CANCEL_TASK] {
        let missing: AgentResponse<TaskView> = proxy
            .call(member, json!([{"id": "t-404", "timeout_ms": 1}]))
            .await?;
        assert_eq!(missing.error.unwrap().code, "NO_SUCH_TASK", "{member}");
    }
    Ok(())
}

#[tokio::test]
async fn a_task_without_jev_fails_with_a_hint_and_reports_what_it_did() -> tinybus::Result<()> {
    use tinycomputer_bus::agent::{AgentResponse, TaskReport, TaskStatus, TaskView};

    let service = service();
    let started = service
        .call(
            &names::methods::START_TASK.try_into()?,
            json!([{"flow": {"app": "Mail", "steps": ["start a new email message"]}}]),
        )
        .await?;
    let started: AgentResponse<TaskView> = serde_json::from_value(started)?;
    let id = started.data.unwrap().id;
    let settled = service
        .call(
            &names::methods::AWAIT_TASK.try_into()?,
            json!([{"id": id, "timeout_ms": 5_000}]),
        )
        .await?;
    let settled: AgentResponse<TaskView> = serde_json::from_value(settled)?;
    assert!(matches!(
        settled.data.unwrap().status,
        TaskStatus::Failed { ref reason, recoverable: false, .. } if reason.contains("Jev")
    ));
    let continued = service
        .call(
            &names::methods::CONTINUE_TASK.try_into()?,
            json!([{"id": id, "approve": true}]),
        )
        .await?;
    let continued: AgentResponse<TaskView> = serde_json::from_value(continued)?;
    assert_eq!(continued.error.unwrap().code, "NOT_WAITING");
    let report = service
        .call(
            &names::methods::TASK_REPORT.try_into()?,
            json!([{"id": id, "trace": false}]),
        )
        .await?;
    let report: AgentResponse<TaskReport> = serde_json::from_value(report)?;
    assert!(report.data.unwrap().flow.is_some());
    Ok(())
}

#[tokio::test]
async fn the_runner_keeps_one_workspace_per_task_until_released() {
    use tinycomputer_bus::agent::TaskId;
    use tinycomputer_engine::FlowRunner;

    let browser = std::sync::Arc::new(tinycomputer_browser::Browser::new(std::sync::Arc::new(
        tinycomputer_browser::AgentBrowser,
    )));
    let runner =
        crate::tinybus_module::runner::WorkspaceRunner::new(crate::Desktop::new(), None, browser);
    let task = TaskId::new("t-1");
    // Nothing observed yet, so there is nothing to read.
    assert_eq!(
        runner.visible_text(&task).await,
        [] as [std::string::String; 0]
    );
    assert_eq!(runner.workspaces.lock().unwrap().len(), 1);
    // No browser session was ever opened, so there is nothing to capture,
    // and a task the runner never saw has nothing either.
    assert!(runner.capture(&task).await.is_none());
    assert!(runner.capture(&TaskId::new("t-unknown")).await.is_none());
    runner.release(&task);
    assert!(runner.workspaces.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_task_report_request_is_one_a_confidential_call_can_carry() -> tinybus::Result<()> {
    use tinycomputer_bus::agent::{TaskId, TaskRef, TaskReportRequest};

    let bus = MemoryBus::new();
    Broker::new().spawn(bus.clone());
    let server = Connection::connect(bus.connect().await?).await?;
    setup(server.clone(), json!({})).await?;
    let client = Connection::connect(bus.connect().await?).await?;
    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;

    // A bare `{"id": ...}` has a stream handle's shape, and the client
    // refuses to put one in a confidential body before anything is sent.
    let bare = proxy
        .call_confidential::<serde_json::Value>(
            names::methods::TASK_REPORT,
            (TaskRef {
                id: TaskId::new("t-1"),
            },),
        )
        .await
        .expect_err("a bare id is refused client-side");
    assert!(bare.to_string().contains("stream handle"), "{bare}");

    // The report's own request always carries `trace`, so it gets past the
    // client and reaches the broker, which refuses it for exactly one
    // reason: this in-memory module was never attested.
    let request = TaskReportRequest::new(TaskId::new("t-1"));
    let refused = proxy
        .call_confidential::<serde_json::Value>(names::methods::TASK_REPORT, (request,))
        .await
        .expect_err("an unattested module cannot take a confidential call");
    assert!(
        matches!(
            &refused,
            tinybus::Error::MethodFailed { name, .. }
                if name.as_str() == "ai.tinyhumans.tinybus.Error.NotAttested"
        ),
        "{refused:?}"
    );
    Ok(())
}
