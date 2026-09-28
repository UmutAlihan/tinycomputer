//! Tests for the linked engine that need no browser.
//!
//! agent-browser launches a browser before running any command not on its
//! skip list, so these send only commands that are on it (such as an empty
//! action): the engine answers in its envelope and nothing is launched.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::AgentBrowser;
use crate::engine::Launcher;

#[tokio::test]
async fn an_empty_command_is_answered_without_launching_a_browser() {
    let mut engine = AgentBrowser.open("unit");
    let reply = engine.execute(json!({"id": "1", "action": ""})).await;
    assert_eq!(reply["success"], false);
    assert!(
        reply["error"]
            .as_str()
            .is_some_and(|error| !error.is_empty())
    );
}
