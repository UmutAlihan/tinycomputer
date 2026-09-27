//! Tests for the `TinyBus` module adapter and its declared surface.
//!
//! These run against the in-memory transport rather than a loaded `cdylib`, so
//! they exercise the dispatch table and the envelope without needing a display
//! server or a granted permission. The `tinydesktop-examples` crate's
//! `verify_module` binary covers the real dynamic loader.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{DesktopService, setup};
use serde_json::json;
use tinybus::broker::Broker;
use tinybus::transport::memory::MemoryBus;
use tinybus::{Connection, Interface};
use tinydesktop_bus::{DesktopResponse, PermissionsRequest, names};

/// The `methods = [...]` list `module_export!` was handed, read back out of
/// this module's own source.
///
/// The macro turns that list into an `extern "C"` function returning a raw
/// slice, and reading one back needs `unsafe`, which this workspace forbids.
/// Reading the literals the macro was given is the same assertion — that the
/// declared manifest and the contract agree — reached the safe way.
fn manifest_methods() -> Vec<String> {
    let source = include_str!("mod.rs");
    let (_, rest) = source
        .split_once("    methods = [")
        .expect("the module declares a methods list");
    let (list, _) = rest
        .split_once("\n    ]")
        .expect("the methods list is closed on its own line");

    list.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

fn service() -> DesktopService {
    DesktopService::from_config(&json!({})).expect("an empty configuration is valid")
}

#[test]
fn declared_methods_match_the_dispatch_table() {
    let methods = service()
        .members()
        .into_iter()
        .map(|member| member.to_string())
        .collect::<Vec<_>>();

    assert_eq!(methods, names::METHODS.to_vec());
}

#[test]
fn the_embedded_manifest_matches_the_contract() {
    assert_eq!(manifest_methods(), names::METHODS.to_vec());
}

#[test]
fn the_served_interface_name_matches_the_contract() {
    assert_eq!(service().name().to_string(), names::INTERFACE);
}

#[test]
fn the_type_member_keeps_the_engines_spelling_rather_than_the_rust_one() {
    // The Rust method is `type_text` because `type` is a keyword; the wire name
    // has to stay `Type` to match the contract a host spells.
    let members = service()
        .members()
        .into_iter()
        .map(|member| member.to_string())
        .collect::<Vec<_>>();

    assert!(members.contains(&"Type".to_owned()));
    assert!(!members.contains(&"TypeText".to_owned()));
}

#[test]
fn a_malformed_configuration_is_rejected_rather_than_defaulted() {
    let error = DesktopService::from_config(&json!({ "session_id": 7 }))
        .expect_err("a numeric session id is not a session id");

    assert!(error.to_string().contains("session_id"));
}

#[tokio::test]
async fn the_module_answers_a_command_over_a_real_bus() -> tinybus::Result<()> {
    let bus = MemoryBus::new();
    Broker::new().spawn(bus.clone());

    let service = Connection::connect(bus.connect().await?).await?;
    setup(service.clone(), json!({})).await?;

    let client = Connection::connect(bus.connect().await?).await?;
    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;
    let reply: DesktopResponse = proxy.call(names::methods::VERSION, ()).await?;

    assert!(reply.ok, "version needs no permission and no display");
    assert_eq!(reply.command, "version");
    assert!(reply.data.is_some());
    Ok(())
}

#[tokio::test]
async fn a_member_taking_a_payload_round_trips_it() -> tinybus::Result<()> {
    let bus = MemoryBus::new();
    Broker::new().spawn(bus.clone());

    let service = Connection::connect(bus.connect().await?).await?;
    setup(service.clone(), json!({})).await?;

    let client = Connection::connect(bus.connect().await?).await?;
    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;
    let reply: DesktopResponse = proxy
        .call(
            names::methods::PERMISSIONS,
            (PermissionsRequest { request: false },),
        )
        .await?;

    // Reporting permissions works everywhere; whether they are granted is the
    // machine's business, not this test's.
    assert_eq!(reply.command, "permissions");
    Ok(())
}

#[tokio::test]
async fn a_reserved_hold_member_fails_closed_over_the_bus() -> tinybus::Result<()> {
    let bus = MemoryBus::new();
    Broker::new().spawn(bus.clone());

    let service = Connection::connect(bus.connect().await?).await?;
    setup(service.clone(), json!({})).await?;

    let client = Connection::connect(bus.connect().await?).await?;
    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;
    let reply: DesktopResponse = proxy
        .call(
            names::methods::KEY_DOWN,
            (json!({ "combo": "shift", "force": false }),),
        )
        .await?;

    // It answers rather than erroring at the transport, and it says no.
    assert!(!reply.ok);
    assert!(reply.error.is_some());
    Ok(())
}

#[tokio::test]
async fn an_unknown_member_is_a_transport_error_not_an_envelope() -> tinybus::Result<()> {
    let bus = MemoryBus::new();
    Broker::new().spawn(bus.clone());

    let service = Connection::connect(bus.connect().await?).await?;
    setup(service.clone(), json!({})).await?;

    let client = Connection::connect(bus.connect().await?).await?;
    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;
    let result = proxy.call::<DesktopResponse>("NoSuchMember", ()).await;

    // The envelope carries command failures; a member that does not exist is a
    // different kind of problem and belongs on the other channel.
    assert!(result.is_err());
    Ok(())
}

/// Every member paired with the positional argument array a caller would send,
/// in the order of [`names::METHODS`].
///
/// The payloads are the same rejected-before-anything-happens ones the engine
/// sweep in `desktop/test.rs` uses, and safe for the same reasons — see the
/// note there. `ClipboardClear` is absent for that note's reason: there is no
/// invalid input to hand it.
fn wire_sweep() -> Vec<(&'static str, serde_json::Value)> {
    let empty_ref = json!([{ "ref_id": "" }]);
    let no_app = json!([{ "app": "" }]);
    let nothing = json!([]);
    let empty = json!([{}]);

    vec![
        (
            names::methods::VALIDATE_FLOW,
            json!([{ "flow": { "app": "Mail", "steps": ["start a new note"] } }]),
        ),
        (names::methods::FLOW_GUIDE, nothing.clone()),
        (names::methods::SNAPSHOT, empty.clone()),
        (names::methods::FIND, empty.clone()),
        (
            names::methods::GET,
            json!([{ "ref_id": "", "property": "text" }]),
        ),
        (
            names::methods::IS,
            json!([{ "ref_id": "", "property": "visible" }]),
        ),
        (names::methods::SCREENSHOT, no_app.clone()),
        (names::methods::CLICK, empty_ref.clone()),
        (names::methods::DOUBLE_CLICK, empty_ref.clone()),
        (names::methods::TRIPLE_CLICK, empty_ref.clone()),
        (names::methods::RIGHT_CLICK, empty_ref.clone()),
        (names::methods::TYPE, empty.clone()),
        (names::methods::SET_VALUE, empty.clone()),
        (names::methods::CLEAR, empty_ref.clone()),
        (names::methods::FOCUS, empty_ref.clone()),
        (names::methods::SELECT, empty.clone()),
        (names::methods::TOGGLE, empty_ref.clone()),
        (names::methods::CHECK, empty_ref.clone()),
        (names::methods::UNCHECK, empty_ref.clone()),
        (names::methods::EXPAND, empty_ref.clone()),
        (names::methods::COLLAPSE, empty_ref.clone()),
        (
            names::methods::SCROLL,
            json!([{ "ref_id": "", "direction": "Down", "amount": 1 }]),
        ),
        (names::methods::SCROLL_TO, empty_ref),
        (names::methods::PRESS, empty.clone()),
        (names::methods::KEY_DOWN, empty.clone()),
        (names::methods::KEY_UP, empty.clone()),
        (names::methods::HOVER, empty.clone()),
        (names::methods::DRAG, empty.clone()),
        (names::methods::MOUSE_MOVE, empty.clone()),
        (names::methods::MOUSE_CLICK, empty.clone()),
        (names::methods::MOUSE_DOWN, empty.clone()),
        (names::methods::MOUSE_UP, empty.clone()),
        (names::methods::MOUSE_WHEEL, empty.clone()),
        (names::methods::LAUNCH, no_app.clone()),
        (names::methods::CLOSE_APP, no_app.clone()),
        (names::methods::LIST_APPS, empty.clone()),
        (names::methods::LIST_WINDOWS, empty.clone()),
        (names::methods::LIST_DISPLAYS, nothing.clone()),
        (names::methods::LIST_SURFACES, no_app.clone()),
        (names::methods::FOCUS_WINDOW, no_app.clone()),
        (
            names::methods::RESIZE_WINDOW,
            json!([{ "app": "", "width": 100.0, "height": 100.0 }]),
        ),
        (names::methods::MOVE_WINDOW, no_app.clone()),
        (names::methods::MINIMIZE, no_app.clone()),
        (names::methods::MAXIMIZE, no_app.clone()),
        (names::methods::RESTORE, no_app),
        (names::methods::CLIPBOARD_GET, empty.clone()),
        (names::methods::CLIPBOARD_SET, empty.clone()),
        (names::methods::LIST_NOTIFICATIONS, empty.clone()),
        (names::methods::NOTIFICATION_ACTION, empty.clone()),
        (names::methods::DISMISS_NOTIFICATION, empty.clone()),
        (names::methods::DISMISS_ALL_NOTIFICATIONS, empty.clone()),
        (names::methods::WAIT, json!([{ "ms": 1 }])),
        (names::methods::VERSION, nothing.clone()),
        (names::methods::STATUS, nothing.clone()),
        (names::methods::PERMISSIONS, empty),
    ]
}

#[test]
fn the_wire_sweep_covers_every_member_except_the_one_with_no_safe_input() {
    let swept = wire_sweep()
        .into_iter()
        .map(|(member, _)| member)
        .collect::<Vec<_>>();
    let missing = names::METHODS
        .iter()
        .filter(|member| !swept.contains(member))
        .collect::<Vec<_>>();

    assert_eq!(
        missing,
        vec![
            &names::methods::RESOLVE_INTENT,
            &names::methods::RUN_GOAL,
            &names::methods::RUN_FLOW,
            &names::methods::DESCRIBE,
            &names::methods::PLAN_TASK,
            &names::methods::START_TASK,
            &names::methods::AWAIT_TASK,
            &names::methods::CONTINUE_TASK,
            &names::methods::CANCEL_TASK,
            &names::methods::TASK_REPORT,
            &names::methods::LIST_TASKS,
            &names::methods::CLIPBOARD_CLEAR,
        ],
        "the task members answer in their own reply shape; see the agent tests below"
    );
}

#[test]
fn every_agentic_member_requires_confidential_delivery() {
    let service = service();
    for member in [
        names::methods::RESOLVE_INTENT,
        names::methods::RUN_GOAL,
        names::methods::RUN_FLOW,
        names::methods::START_TASK,
        names::methods::CONTINUE_TASK,
        names::methods::TASK_REPORT,
    ] {
        assert!(
            service.requires_confidential(&member.try_into().expect("valid member")),
            "{member} was ordinary"
        );
    }
    assert!(
        !service.requires_confidential(
            &names::methods::VERSION
                .try_into()
                .expect("valid ordinary member")
        )
    );
}

#[tokio::test]
async fn private_module_configuration_initializes_jev_without_exposing_the_key()
-> tinybus::Result<()> {
    // The provider's default endpoint: loopback is trusted only inside the
    // engine's own tests. The missing application fails observation before
    // any Jev request, so nothing here reaches the network.
    let configured_service = DesktopService::from_config(&json!({
        "jev": {
            "api_key": "test-secret",
            "provider": "open_router",
            "model": "jev-test",
            "max_retries": 0
        }
    }))
    .expect("private Jev configuration is valid");
    let resolved = configured_service
        .call(
            &names::methods::RESOLVE_INTENT.try_into()?,
            json!([{"app": "__tinydesktop_missing__", "intent": "click"}]),
        )
        .await?;
    let resolved: DesktopResponse = serde_json::from_value(resolved)?;
    assert!(!resolved.ok);
    assert!(!format!("{resolved:?}").contains("test-secret"));

    let service = service();
    for (member, body) in [
        (
            names::methods::RESOLVE_INTENT,
            json!([{"app": "App", "intent": "click something"}]),
        ),
        (
            names::methods::RUN_GOAL,
            json!([{"app": "App", "goal": "finish"}]),
        ),
        (
            names::methods::RUN_FLOW,
            json!([{"flow": {"app": "App", "steps": ["finish"]}}]),
        ),
    ] {
        let reply = service.call(&member.try_into()?, body).await?;
        let reply: DesktopResponse = serde_json::from_value(reply)?;
        assert_eq!(
            reply.error.expect("unconfigured reply").code,
            "JEV_NOT_CONFIGURED"
        );
    }
    assert!(DesktopService::from_config(&json!({"jev": {"api_key": 7}})).is_err());
    Ok(())
}

#[tokio::test]
async fn every_member_decodes_its_payload_and_answers_in_the_envelope() -> tinybus::Result<()> {
    let bus = MemoryBus::new();
    Broker::new().spawn(bus.clone());

    let service = Connection::connect(bus.connect().await?).await?;
    setup(service.clone(), json!({})).await?;

    let client = Connection::connect(bus.connect().await?).await?;
    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;

    for (member, body) in wire_sweep() {
        // A decode failure here would be a transport error rather than an
        // envelope, so reaching the envelope at all is half the assertion.
        let reply: DesktopResponse = proxy.call(member, body).await?;

        assert!(!reply.command.is_empty(), "{member} named nothing");
        assert_eq!(reply.ok, reply.data.is_some(), "{member}");
        assert_eq!(!reply.ok, reply.error.is_some(), "{member}");
    }
    Ok(())
}

#[tokio::test]
async fn the_ordinary_task_members_answer_over_a_real_bus() -> tinybus::Result<()> {
    use tinydesktop_bus::agent::{AgentResponse, Capabilities, TaskView};

    let bus = MemoryBus::new();
    Broker::new().spawn(bus.clone());
    let service = Connection::connect(bus.connect().await?).await?;
    setup(service.clone(), json!({})).await?;
    let client = Connection::connect(bus.connect().await?).await?;
    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;

    let described: Capabilities = proxy.call(names::methods::DESCRIBE, json!([])).await?;
    assert!(!described.jev_configured);
    assert_eq!(described.members.len(), 8);
    assert!(described.surfaces.iter().any(|surface| !surface.available));

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
    use tinydesktop_bus::agent::{AgentResponse, TaskReport, TaskStatus, TaskView};

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
            json!([{"id": id}]),
        )
        .await?;
    let report: AgentResponse<TaskReport> = serde_json::from_value(report)?;
    assert!(report.data.unwrap().flow.is_some());
    Ok(())
}

#[test]
fn the_desktop_is_available_only_with_accessibility() {
    use super::dispatch::desktop_availability;

    let reply = |state: serde_json::Value| {
        DesktopResponse::ok("permissions", json!({"accessibility": state}))
    };
    assert!(desktop_availability(&reply(json!({"state": "granted"}))).available);
    assert!(desktop_availability(&reply(json!({"state": "not_required"}))).available);
    let denied = desktop_availability(&reply(
        json!({"state": "denied", "suggestion": "open Settings"}),
    ));
    assert!(!denied.available);
    assert_eq!(denied.reason.as_deref(), Some("open Settings"));
    let bare = desktop_availability(&reply(json!({"state": "denied"})));
    assert_eq!(
        bare.reason.as_deref(),
        Some("grant the accessibility permission")
    );
    let unknown = desktop_availability(&reply(json!({"state": "unknown"})));
    assert!(unknown.reason.unwrap().contains("could not be read"));
    let failed = desktop_availability(&DesktopResponse::err(
        "permissions",
        tinydesktop_bus::DesktopError::new("PLATFORM_NOT_SUPPORTED", "no surfaces here"),
    ));
    assert_eq!(failed.reason.as_deref(), Some("no surfaces here"));
}

#[test]
fn a_planner_is_configured_from_private_configuration_only_with_a_key() {
    assert!(DesktopService::from_config(&json!({"planner": {"api_key": "k"}})).is_ok());
    assert!(DesktopService::from_config(&json!({"planner": {"api_key": " "}})).is_err());
    assert!(DesktopService::from_config(&json!({"planner": {"model": "m"}})).is_err());
}

#[tokio::test]
async fn the_runner_keeps_one_workspace_per_task_until_released() {
    use tinydesktop_bus::agent::TaskId;
    use tinydesktop_engine::FlowRunner;

    let runner = super::runner::WorkspaceRunner::new(crate::Desktop::new(), None);
    let task = TaskId::new("t-1");
    // Nothing observed yet, so there is nothing to read.
    assert!(runner.visible_text(&task).await.is_empty());
    assert_eq!(runner.workspaces.lock().unwrap().len(), 1);
    runner.release(&task);
    assert!(runner.workspaces.lock().unwrap().is_empty());
}
