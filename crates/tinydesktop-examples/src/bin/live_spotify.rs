//! Opt-in real-module Jev exercise against Spotify on macOS.
//!
//! This spends `OpenRouter` credit and changes Spotify's visible navigation
//! state. The module artifact must be attested by a
//! `modules.toml` beside it so `TinyBus` will attest confidential goal calls.
//! The API key itself arrives through sensitive module initialization.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use serde_json::Value;
use tinybus::{Connection, broker::Broker, module::ModuleHost, transport::memory::MemoryBus};
use tinydesktop_bus::{
    DesktopResponse, FindRequest, FocusWindowRequest, JevConfig, JevProvider, JevRunResult,
    JevStopReason, LaunchRequest, RefRequest, RunGoalRequest, SnapshotRequest, names,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let module = module_argument()?;
    verify_allowlisted(&module)?;
    let key = std::env::var("OPENROUTER_API_KEY")
        .map_err(|_| io::Error::other("OPENROUTER_API_KEY is not exported"))?;

    let bus = MemoryBus::new();
    let broker = Broker::new();
    let broker_task = broker.spawn(bus.clone());
    let module_host = ModuleHost::new(broker);
    let info = module_host.load_file(&module)?;
    if info.name != "tinydesktop" {
        return Err(io::Error::other(format!("loaded unexpected module `{}`", info.name)).into());
    }

    let client = Connection::connect(bus.connect().await?).await?;
    wait_for_module(&client).await?;
    let mut jev = JevConfig::new(key);
    jev.provider = JevProvider::OpenRouter;
    jev.endpoint_url = Some("https://openrouter.ai/api/alpha/decisions".to_owned());
    jev.model = Some("jev-latest".to_owned());
    client
        .reinitialize_module("tinydesktop", serde_json::json!({"jev": jev}))
        .await?;
    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;
    if proxy.attestation().await?.is_none() {
        return Err(io::Error::other(
            "tinydesktop is not attested; generate modules.toml beside the built library",
        )
        .into());
    }

    exercise(&proxy).await?;
    broker_task.abort();
    Ok(())
}

async fn exercise(proxy: &tinybus::Proxy) -> Result<(), Box<dyn std::error::Error>> {
    let launched = launch_spotify(proxy).await?;
    println!("Spotify launch is ready");
    let launch_window = launched.data.as_ref().and_then(|data| data.get("window"));
    if !launch_window
        .and_then(|window| window.get("visible"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && let Some(window_id) = launch_window
            .and_then(|window| window.get("id"))
            .and_then(Value::as_str)
    {
        let _: DesktopResponse = proxy
            .call(
                names::methods::FOCUS_WINDOW,
                (FocusWindowRequest {
                    window_id: Some(window_id.to_owned()),
                    ..FocusWindowRequest::default()
                },),
            )
            .await?;
    }
    wait_for_spotify(proxy).await?;
    pause_if_playing(proxy).await?;
    let _ = launch_spotify(proxy).await?;
    wait_for_spotify(proxy).await?;

    let run: DesktopResponse = proxy
        .call_confidential(
            names::methods::RUN_GOAL,
            (RunGoalRequest {
                app: "Spotify".to_owned(),
                goal: "Open Liked Songs and ensure the topmost song is playing; choose DONE if it is already playing"
                    .to_owned(),
                max_steps: 12,
                max_model_calls: 20,
                ..RunGoalRequest::default()
            },),
        )
        .await?;
    ensure_ok("RunGoal", &run)?;
    let result: JevRunResult = serde_json::from_value(
        run.data
            .clone()
            .ok_or_else(|| io::Error::other("RunGoal returned no data"))?,
    )?;
    report(&result);
    if result.stop != JevStopReason::Done {
        return Err(io::Error::other(format!(
            "Spotify goal stopped as {:?} instead of Done",
            result.stop
        ))
        .into());
    }

    verify_playback(proxy).await?;

    Ok(())
}

fn report(result: &JevRunResult) {
    println!(
        "Spotify Jev run: stop={:?}, steps={}, calls={}, attempts={}, latency_ms={}, model={}",
        result.stop,
        result.turns.len(),
        result.metrics.calls,
        result.metrics.attempts,
        result.metrics.latency_ms,
        result.metrics.model.as_deref().unwrap_or("unknown")
    );
    if let Some(pending) = &result.pending {
        println!(
            "pending: decision={:?}, operation={:?}, target={:?}, confidence={:.2}, destructive={:.2}, reason={}",
            pending.decision,
            pending.operation,
            pending.target,
            pending.confidence,
            pending.destructive,
            pending.reason
        );
    }
}

async fn verify_playback(proxy: &tinybus::Proxy) -> Result<(), Box<dyn std::error::Error>> {
    let found: DesktopResponse = proxy
        .call(
            names::methods::FIND,
            (FindRequest {
                app: Some("Spotify".to_owned()),
                role: Some("button".to_owned()),
                name: Some("Pause".to_owned()),
                exact: true,
                first: true,
                ..FindRequest::default()
            },),
        )
        .await?;
    if !found.ok || found.data.as_ref().and_then(first_ref).is_none() {
        return Err(io::Error::other(
            "Spotify did not expose a Pause control, so active playback was not verified",
        )
        .into());
    }
    Ok(())
}

async fn pause_if_playing(proxy: &tinybus::Proxy) -> Result<(), Box<dyn std::error::Error>> {
    let found: DesktopResponse = proxy
        .call(
            names::methods::FIND,
            (FindRequest {
                app: Some("Spotify".to_owned()),
                role: Some("button".to_owned()),
                name: Some("Pause".to_owned()),
                exact: true,
                first: true,
                ..FindRequest::default()
            },),
        )
        .await?;
    let Some(ref_id) = found.data.as_ref().and_then(first_ref) else {
        return Ok(());
    };
    let paused: DesktopResponse = proxy
        .call(names::methods::CLICK, (RefRequest::new(ref_id),))
        .await?;
    ensure_ok("Pause", &paused)?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    Ok(())
}

async fn wait_for_module(connection: &Connection) -> tinybus::Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if connection
                .list_names()
                .await?
                .iter()
                .any(|name| name.as_str() == names::INTERFACE)
            {
                return Ok(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| tinybus::Error::failed("timed out waiting for tinydesktop"))?
}

async fn wait_for_spotify(proxy: &tinybus::Proxy) -> Result<(), Box<dyn std::error::Error>> {
    let mut reported_error = false;
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let reply: DesktopResponse = proxy
                .call(
                    names::methods::SNAPSHOT,
                    (SnapshotRequest {
                        app: Some("Spotify".to_owned()),
                        skeleton: true,
                        ..SnapshotRequest::default()
                    },),
                )
                .await?;
            if reply.ok {
                return tinybus::Result::Ok(());
            }
            if !reported_error {
                if let Some(error) = &reply.error {
                    eprintln!(
                        "Spotify snapshot preflight: {}: {}",
                        error.code, error.message
                    );
                }
                reported_error = true;
            }
            let _: DesktopResponse = proxy
                .call(
                    names::methods::LAUNCH,
                    (LaunchRequest {
                        activate: true,
                        ..LaunchRequest::new("Spotify")
                    },),
                )
                .await?;
            let _: DesktopResponse = proxy
                .call(
                    names::methods::FOCUS_WINDOW,
                    (FocusWindowRequest {
                        app: Some("Spotify".to_owned()),
                        ..FocusWindowRequest::default()
                    },),
                )
                .await?;
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await
    .map_err(|_| {
        io::Error::other("Spotify did not expose an accessibility tree in 15 seconds")
    })??;
    Ok(())
}

async fn launch_spotify(
    proxy: &tinybus::Proxy,
) -> Result<DesktopResponse, Box<dyn std::error::Error>> {
    let reply = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let reply: DesktopResponse = proxy
                .call(
                    names::methods::LAUNCH,
                    (LaunchRequest {
                        activate: true,
                        ..LaunchRequest::new("Spotify")
                    },),
                )
                .await?;
            if reply.ok {
                return tinybus::Result::Ok(reply);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await
    .map_err(|_| io::Error::other("Spotify launch did not settle within 15 seconds"))??;
    Ok(reply)
}

fn ensure_ok(name: &str, reply: &DesktopResponse) -> Result<(), io::Error> {
    if reply.ok {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{name} failed: {}",
            reply.error.as_ref().map_or_else(
                || "unknown error".to_owned(),
                |error| format!("{}: {}", error.code, error.message),
            )
        )))
    }
}

fn first_ref(value: &Value) -> Option<&str> {
    match value {
        Value::Object(values) => values
            .get("ref_id")
            .and_then(Value::as_str)
            .or_else(|| values.values().find_map(first_ref)),
        Value::Array(values) => values.iter().find_map(first_ref),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => None,
    }
}

fn verify_allowlisted(module: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file_name = module
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::other("module path has no UTF-8 file name"))?;
    let manifest_path = module
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("modules.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .map_err(|_| io::Error::other("modules.toml is required before loading the module"))?;
    let prefix = format!("\"{file_name}\" = \"");
    let expected = manifest
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| io::Error::other("module is absent from modules.toml"))?;
    let observed = tinybus::module::sha256_file(module)?;
    if observed != expected {
        return Err(io::Error::other("module checksum does not match modules.toml").into());
    }
    Ok(())
}

fn module_argument() -> Result<PathBuf, io::Error> {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("usage: live_spotify <path-to-attested-module>"))
}
