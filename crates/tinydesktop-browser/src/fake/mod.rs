//! A scripted engine for tests: records every command and answers the way
//! agent-browser does, writing screenshot and download files where asked.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::engine::{Engine, Launcher, Reply};

type Script = dyn Fn(&Value) -> Option<Value> + Send + Sync;

/// Records every command; answers from an optional override, else like the
/// engine would.
#[derive(Clone)]
pub(crate) struct Fake {
    sent: Arc<Mutex<Vec<Value>>>,
    script: Arc<Script>,
}

impl std::fmt::Debug for Fake {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Fake")
    }
}

impl Fake {
    pub(crate) fn new() -> Self {
        Self::scripted(|_| None)
    }

    pub(crate) fn scripted(script: impl Fn(&Value) -> Option<Value> + Send + Sync + 'static) -> Self {
        Self {
            sent: Arc::new(Mutex::new(Vec::new())),
            script: Arc::new(script),
        }
    }

    pub(crate) fn actions(&self) -> Vec<String> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .map(|command| command["action"].as_str().unwrap().to_owned())
            .collect()
    }

    pub(crate) fn last(&self, action: &str) -> Value {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|command| command["action"] == action)
            .cloned()
            .unwrap_or_else(|| panic!("{action} was never sent"))
    }
}

pub(crate) fn ok(data: &Value) -> Value {
    json!({"id": "", "success": true, "data": data})
}

pub(crate) fn failure(message: &str) -> Value {
    json!({"id": "", "success": false, "error": message})
}

pub(crate) fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes
}

pub(crate) fn default_reply(command: &Value) -> Value {
    let path = command["path"].as_str().map(str::to_owned);
    match command["action"].as_str().unwrap() {
        "url" => ok(&json!({"url": "https://flights.test/"})),
        "title" => ok(&json!({"title": "Flights"})),
        "navigate" => ok(&json!({"url": command["url"], "title": "Loaded"})),
        "snapshot" => ok(&json!({
            "snapshot": "- button \"Search\" [ref=e1]",
            "refs": {"e1": {"role": "button", "name": "Search"}}
        })),
        "gettext" => ok(&json!({"text": "IndiGo ₹6,840"})),
        "getattribute" => ok(&json!({"value": "/book"})),
        "isvisible" => ok(&json!({"visible": true})),
        "read" => ok(&json!({"content": "# Flights\nIndiGo", "truncated": false})),
        "content" => ok(&json!({"html": "<h1>Flights</h1>"})),
        "evaluate" => ok(&json!({"result": 42})),
        "screenshot" => {
            std::fs::write(path.as_ref().unwrap(), png(1280, 800)).unwrap();
            ok(&json!({"path": path}))
        }
        "waitfordownload" => {
            std::fs::write(path.as_ref().unwrap(), b"itinerary").unwrap();
            ok(&json!({"path": path}))
        }
        _ => ok(&json!({})),
    }
}

impl Engine for Fake {
    fn execute(&mut self, command: Value) -> Reply<'_> {
        self.sent.lock().unwrap().push(command.clone());
        let reply = (self.script)(&command).unwrap_or_else(|| default_reply(&command));
        Box::pin(async move { reply })
    }
}

impl Launcher for Fake {
    fn open(&self, _session: &str) -> Box<dyn Engine> {
        Box::new(self.clone())
    }
}
