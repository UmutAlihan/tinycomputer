//! Opt-in, direct `TinyBus` exercise of a bounded Calculator goal.
//!
//! Run with `OPENROUTER_API_KEY` and an attested module path. The `probe` mode
//! prints Calculator's accessibility controls without contacting Jev; `run`
//! performs a scoped calculation and verifies the visible result.

use std::{ffi::OsStr, io, path::PathBuf, time::Duration};

use serde_json::Value;
use tinybus::{Connection, broker::Broker, module::ModuleHost, transport::memory::MemoryBus};
use tinydesktop_bus::{
    DesktopResponse, FindRequest, FocusWindowRequest, JevConfig, JevOperation, JevProvider,
    JevRunResult, JevStopReason, LaunchRequest, RefRequest, RunGoalRequest, SnapshotRequest,
    VisiblePredicate, names,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let mode = arguments
        .next()
        .ok_or("usage: live_goal <probe|run> <attested-module>")?;
    validate_mode(&mode)?;
    let module = PathBuf::from(arguments.next().ok_or("missing module path")?);
    let bus = MemoryBus::new();
    let broker = Broker::new();
    let broker_task = broker.spawn(bus.clone());
    let host = ModuleHost::new(broker);
    let info = host.load_file(&module)?;
    if info.name != "tinydesktop" {
        return Err(io::Error::other("unexpected module").into());
    }
    let client = Connection::connect(bus.connect().await?).await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if client
                .list_names()
                .await?
                .iter()
                .any(|name| name.as_str() == names::INTERFACE)
            {
                return tinybus::Result::Ok(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;
    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;
    if proxy.attestation().await?.is_none() {
        return Err(io::Error::other("module is not attested").into());
    }
    if mode == "run" {
        let key = std::env::var("OPENROUTER_API_KEY")
            .map_err(|_| io::Error::other("OPENROUTER_API_KEY is required"))?;
        let mut jev = JevConfig::new(key);
        jev.provider = JevProvider::OpenRouter;
        jev.model = Some("jev-latest".to_owned());
        client
            .reinitialize_module("tinydesktop", serde_json::json!({"jev": jev}))
            .await?;
    }
    let launched: DesktopResponse = proxy
        .call(
            names::methods::LAUNCH,
            (LaunchRequest {
                activate: true,
                ..LaunchRequest::new("Calculator")
            },),
        )
        .await?;
    require_ok("launch", &launched)?;
    let snapshot = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let reply: DesktopResponse = proxy
                .call(
                    names::methods::SNAPSHOT,
                    (SnapshotRequest {
                        app: Some("Calculator".to_owned()),
                        ..SnapshotRequest::default()
                    },),
                )
                .await?;
            if reply.ok {
                return tinybus::Result::Ok(reply);
            }
            let _: DesktopResponse = proxy
                .call(
                    names::methods::FOCUS_WINDOW,
                    (FocusWindowRequest {
                        app: Some("Calculator".to_owned()),
                        ..FocusWindowRequest::default()
                    },),
                )
                .await?;
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await
    .map_err(|_| io::Error::other("Calculator did not expose a window in 30 seconds"))??;
    require_ok("snapshot", &snapshot)?;
    let tree = snapshot.data.as_ref().and_then(|data| data.get("tree"));
    if mode == "probe" {
        print_nodes(tree, 0);
        broker_task.abort();
        return Ok(());
    }
    run_calculation(&proxy).await?;
    broker_task.abort();
    Ok(())
}

fn validate_mode(mode: &OsStr) -> Result<(), io::Error> {
    if mode == "probe" || mode == "run" {
        Ok(())
    } else {
        Err(io::Error::other("mode must be probe or run"))
    }
}

async fn run_calculation(proxy: &tinybus::Proxy) -> Result<(), Box<dyn std::error::Error>> {
    reset_calculator(proxy).await?;
    let reply: DesktopResponse = proxy.call_confidential(names::methods::RUN_GOAL, (
        RunGoalRequest {
            app: "Calculator".to_owned(),
            goal: "Calculate 7 + 5 = 12 using the visible Calculator buttons, then stop when the result display shows 12".to_owned(),
            allowed_operations: vec![JevOperation::Click],
            allowed_targets: vec!["7".to_owned(), "Add".to_owned(), "5".to_owned(), "Equals".to_owned()],
            success: vec![VisiblePredicate::NamePresent { name: "\u{200e}12".to_owned() }],
            max_steps: 6,
            max_model_calls: 12,
            max_elapsed_ms: 90_000,
            require_confirmations: false,
            ..RunGoalRequest::default()
        },
    )).await?;
    require_ok("RunGoal", &reply)?;
    let result: JevRunResult = serde_json::from_value(reply.data.ok_or("missing goal result")?)?;
    println!(
        "stop={:?} verified={} steps={} calls={}",
        result.stop,
        result.verified,
        result.turns.len(),
        result.metrics.calls
    );
    for turn in &result.turns {
        println!(
            "step={} operation={:?} target={:?} ok={} changed={}",
            turn.step,
            turn.operation,
            turn.target
                .as_ref()
                .and_then(|target| target.name.as_deref()),
            turn.ok,
            turn.changed
        );
    }
    let after: DesktopResponse = proxy
        .call(
            names::methods::SNAPSHOT,
            (SnapshotRequest {
                app: Some("Calculator".to_owned()),
                ..SnapshotRequest::default()
            },),
        )
        .await?;
    require_ok("final snapshot", &after)?;
    if result.stop != JevStopReason::Done || !result.verified {
        print_nodes(after.data.as_ref().and_then(|data| data.get("tree")), 0);
        return Err(io::Error::other("Calculator goal did not verify").into());
    }
    Ok(())
}

fn require_ok(name: &str, response: &DesktopResponse) -> Result<(), io::Error> {
    if response.ok {
        return Ok(());
    }
    Err(io::Error::other(format!("{name}: {:?}", response.error)))
}

async fn reset_calculator(proxy: &tinybus::Proxy) -> Result<(), Box<dyn std::error::Error>> {
    for label in ["All Clear", "Clear"] {
        let found: DesktopResponse = proxy
            .call(
                names::methods::FIND,
                (FindRequest {
                    app: Some("Calculator".to_owned()),
                    role: Some("button".to_owned()),
                    name: Some(label.to_owned()),
                    exact: true,
                    first: true,
                    ..FindRequest::default()
                },),
            )
            .await?;
        if let Some(reference) = found.data.as_ref().and_then(first_ref) {
            let clicked: DesktopResponse = proxy
                .call(names::methods::CLICK, (RefRequest::new(reference),))
                .await?;
            require_ok("reset Calculator", &clicked)?;
            return Ok(());
        }
    }
    Err(io::Error::other("Calculator has no clear button").into())
}

fn first_ref(value: &Value) -> Option<&str> {
    match value {
        Value::Object(values) => values
            .get("ref_id")
            .and_then(Value::as_str)
            .or_else(|| values.values().find_map(first_ref)),
        Value::Array(values) => values.iter().find_map(first_ref),
        _ => None,
    }
}

fn print_nodes(node: Option<&Value>, depth: usize) {
    let Some(node) = node else {
        return;
    };
    if depth > 8 {
        return;
    }
    let name = node.get("name").and_then(Value::as_str).unwrap_or("");
    let role = node.get("role").and_then(Value::as_str).unwrap_or("");
    let value = node.get("value").and_then(Value::as_str).unwrap_or("");
    let reference = node.get("ref_id").and_then(Value::as_str).unwrap_or("");
    let actions = node
        .get("available_actions")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if !name.is_empty() || !value.is_empty() || actions > 0 {
        println!(
            "{depth}: role={role:?} name={name:?} value={value:?} ref={reference:?} actions={actions}"
        );
    }
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for child in children {
            print_nodes(Some(child), depth + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::validate_mode;
    use std::ffi::OsStr;

    #[test]
    fn mode_is_checked_before_module_setup() {
        assert!(validate_mode(OsStr::new("probe")).is_ok());
        assert!(validate_mode(OsStr::new("run")).is_ok());
        assert!(validate_mode(OsStr::new("typo")).is_err());
    }
}
