//! Tests for the browser members, over the in-memory bus.
//!
//! The service's [`Browser`] runs on a scripted engine rather than a linked
//! Chrome, so these check what the bus adds — the one-object requests, the
//! shared envelope, and the error codes and recovery hints — on a machine
//! with no browser installed. Chrome itself is exercised in the Docker lab.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tinybus::broker::Broker;
use tinybus::transport::memory::MemoryBus;
use tinybus::{Connection, Proxy};
use tinycomputer_browser::{Browser, Engine, Launcher, Reply};
use tinycomputer_bus::browser::names::methods;
use tinycomputer_bus::{DeliveryDisposition, DesktopResponse, names};

use super::super::DesktopService;

/// An engine that answers each command the way agent-browser would for a
/// page at `https://example.com/`, refuses the ref `e9` as stale, and records
/// every command it is sent.
struct Scripted(Arc<Mutex<Vec<Value>>>);

impl Engine for Scripted {
    fn execute(&mut self, command: Value) -> Reply<'_> {
        let reply = answer(&command);
        self.0.lock().expect("the log is not poisoned").push(command);
        Box::pin(async move { reply })
    }
}

fn answer(command: &Value) -> Value {
    let page = json!({"url": "https://example.com/", "title": "Example"});
    match command["action"].as_str() {
        Some("navigate") => json!({"success": true, "data": {
            "url": command["url"], "title": "Example"
        }}),
        Some("screenshot") => {
            let path = command["path"].as_str().expect("a screenshot names its path");
            std::fs::write(path, b"scripted image").expect("the scratch file is writable");
            json!({"success": true, "data": {"path": path}})
        }
        Some("click") if command["selector"] == "@e9" => {
            json!({"success": false, "error": "Unknown ref: e9"})
        }
        Some("url") => json!({"success": true, "data": {"url": page["url"]}}),
        Some("title") => json!({"success": true, "data": {"title": page["title"]}}),
        _ => json!({"success": true, "data": page}),
    }
}

#[derive(Debug, Default)]
struct ScriptedLauncher(Arc<Mutex<Vec<Value>>>);

impl Launcher for ScriptedLauncher {
    fn open(&self, _session: &str) -> Box<dyn Engine> {
        Box::new(Scripted(self.0.clone()))
    }
}

/// A private scratch directory for one test, removed when it drops.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "tinycomputer-bus-browser-{name}-{}",
            std::process::id()
        ));
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _removed = std::fs::remove_dir_all(&self.0);
    }
}

/// Serves a scripted-browser service on a fresh bus and returns a proxy to it.
async fn serve(scratch: &Scratch) -> tinybus::Result<Proxy> {
    serve_with(scratch, &json!({}), Arc::new(ScriptedLauncher::default())).await
}

/// Serves a service configured by `config`, whose browser opens sessions on
/// `launcher`.
async fn serve_with(
    scratch: &Scratch,
    config: &Value,
    launcher: Arc<ScriptedLauncher>,
) -> tinybus::Result<Proxy> {
    let bus = MemoryBus::new();
    Broker::new().spawn(bus.clone());

    let browser = Arc::new(Browser::with_scratch(launcher, scratch.0.clone()));
    let service = DesktopService::with_browser(config, browser).expect("the config is valid");
    let server = Connection::connect(bus.connect().await?).await?;
    server
        .serve_at(names::OBJECT_PATH.try_into()?, service)
        .await?;
    server.request_name(names::INTERFACE).await?;

    let client = Connection::connect(bus.connect().await?).await?;
    client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)
}

async fn call(proxy: &Proxy, member: &str, request: Value) -> tinybus::Result<DesktopResponse> {
    proxy.call(member, (request,)).await
}

fn data(reply: &DesktopResponse) -> &Value {
    assert!(reply.ok, "{} failed: {:?}", reply.command, reply.error);
    reply.data.as_ref().expect("a successful reply carries data")
}

#[tokio::test]
async fn a_session_runs_open_navigate_click_and_close_over_the_bus() -> tinybus::Result<()> {
    let scratch = Scratch::new("loop");
    let proxy = serve(&scratch).await?;

    let opened = call(&proxy, methods::OPEN_SESSION, json!({})).await?;
    assert_eq!(opened.command, "browser-open-session");
    let session = data(&opened)["id"].clone();

    let navigated = call(
        &proxy,
        methods::NAVIGATE,
        json!({"session": session, "url": "https://example.com/next"}),
    )
    .await?;
    assert_eq!(data(&navigated)["url"], "https://example.com/next");

    let clicked = call(
        &proxy,
        methods::PERFORM,
        json!({"session": session, "action": "click", "target": {"kind": "ref", "value": "e3"}}),
    )
    .await?;
    assert_eq!(clicked.command, "browser-perform");
    assert!(clicked.ok);

    let listed: DesktopResponse = proxy.call(methods::LIST_SESSIONS, ()).await?;
    assert_eq!(data(&listed).as_array().map(Vec::len), Some(1));

    let closed = call(&proxy, methods::CLOSE_SESSION, json!({"session": session})).await?;
    assert!(closed.ok);
    let again = call(&proxy, methods::CLOSE_SESSION, json!({"session": session})).await?;
    assert!(again.ok, "closing a closed session succeeds");
    Ok(())
}

#[tokio::test]
async fn a_screenshot_is_read_back_in_chunks_and_released() -> tinybus::Result<()> {
    let scratch = Scratch::new("shot");
    let proxy = serve(&scratch).await?;
    let opened = call(&proxy, methods::OPEN_SESSION, json!({})).await?;
    let session = data(&opened)["id"].clone();

    let shot = call(&proxy, methods::SCREENSHOT, json!({"session": session})).await?;
    let output = data(&shot)["id"].clone();
    assert_eq!(data(&shot)["total_bytes"], 14);

    let first = call(
        &proxy,
        methods::READ_OUTPUT,
        json!({"output": output, "max_len": 8}),
    )
    .await?;
    assert_eq!(data(&first)["eof"], false);
    let rest = call(
        &proxy,
        methods::READ_OUTPUT,
        json!({"output": output, "offset": 8}),
    )
    .await?;
    assert_eq!(data(&rest)["eof"], true);

    let released = call(&proxy, methods::RELEASE_OUTPUT, json!({"output": output})).await?;
    assert!(released.ok);
    let gone = call(&proxy, methods::READ_OUTPUT, json!({"output": output})).await?;
    let error = gone.error.expect("a released output is gone");
    assert_eq!(error.code, "OUTPUT_NOT_FOUND");
    assert!(error.suggestion.is_some());
    Ok(())
}

#[tokio::test]
async fn a_stale_ref_fails_with_the_desktop_code_and_recovery() -> tinybus::Result<()> {
    let scratch = Scratch::new("stale");
    let proxy = serve(&scratch).await?;
    let opened = call(&proxy, methods::OPEN_SESSION, json!({})).await?;
    let session = data(&opened)["id"].clone();

    let reply = call(
        &proxy,
        methods::PERFORM,
        json!({"session": session, "action": "click", "target": {"kind": "ref", "value": "e9"}}),
    )
    .await?;
    assert!(!reply.ok);
    assert_eq!(reply.command, "browser-perform");
    let error = reply.error.expect("a failed reply carries its error");
    assert_eq!(error.code, "STALE_REF");
    let hint = error.recovery.expect("a stale ref has a way out");
    assert_eq!(hint.strategy, "refresh_snapshot_then_retry_original");
    assert!(hint.requires_fresh_snapshot);
    assert_eq!(
        error.details.expect("details carry the wire name")["name"],
        tinycomputer_bus::browser::errors::STALE_REF
    );
    Ok(())
}

#[tokio::test]
async fn an_unknown_session_is_refused_before_anything_is_sent() -> tinybus::Result<()> {
    let scratch = Scratch::new("unknown");
    let proxy = serve(&scratch).await?;

    for (member, request) in [
        (methods::NAVIGATE, json!({"session": "s-404", "url": "https://example.com"})),
        (methods::SNAPSHOT, json!({"session": "s-404"})),
        (methods::READ_PAGE, json!({"session": "s-404"})),
        (methods::EVALUATE, json!({"session": "s-404", "expression": "1"})),
        (methods::LIST_DOWNLOADS, json!({"session": "s-404"})),
        (methods::WAIT_DOWNLOAD, json!({"session": "s-404"})),
    ] {
        let reply = call(&proxy, member, request).await?;
        let error = reply.error.expect("an unknown session fails");
        assert_eq!(error.code, "SESSION_NOT_FOUND", "{member}");
        assert_eq!(
            error.disposition.delivery,
            DeliveryDisposition::NotDelivered,
            "{member}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_open_session_takes_the_configured_executable() -> tinybus::Result<()> {
    let scratch = Scratch::new("executable");
    let launcher = Arc::new(ScriptedLauncher::default());
    let proxy = serve_with(
        &scratch,
        &json!({"browser": {"executable": "/opt/chromium"}}),
        launcher.clone(),
    )
    .await?;

    assert!(call(&proxy, methods::OPEN_SESSION, json!({})).await?.ok);
    let attached = call(
        &proxy,
        methods::OPEN_SESSION,
        json!({"endpoint": "http://127.0.0.1:9222"}),
    )
    .await?;
    assert_eq!(data(&attached)["launched"], false);

    let launches = launcher
        .0
        .lock()
        .expect("the log is not poisoned")
        .iter()
        .filter(|command| command["action"] == "launch")
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(launches.len(), 2);
    assert_eq!(launches[0]["executablePath"], "/opt/chromium");
    assert!(
        launches[1].get("executablePath").is_none(),
        "an attached session launches nothing, so it takes no executable"
    );
    Ok(())
}

#[tokio::test]
async fn a_malformed_request_is_a_bus_error_not_a_panic() -> tinybus::Result<()> {
    let scratch = Scratch::new("malformed");
    let proxy = serve(&scratch).await?;
    let reply = call(&proxy, methods::NAVIGATE, json!({"url": "https://example.com"})).await;
    assert!(reply.is_err(), "a request with no session does not decode");
    Ok(())
}
