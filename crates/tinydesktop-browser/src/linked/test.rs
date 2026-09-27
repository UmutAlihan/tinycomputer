//! Tests for the linked engine that need no browser: a command the engine
//! does not know is answered in its envelope without launching anything.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::AgentBrowser;
use crate::engine::Launcher;

#[tokio::test]
async fn an_unknown_command_is_answered_without_launching_a_browser() {
    let mut engine = AgentBrowser.open("unit");
    let reply = engine
        .execute(json!({"id": "1", "action": "unknown-test-command"}))
        .await;
    assert_eq!(reply["success"], false);
    assert!(reply["error"].as_str().is_some_and(|error| !error.is_empty()));
}
