//! Tests for running surface calls off the async executor.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tinydesktop_bus::DesktopResponse;

use super::{blocking, observe_async};
use crate::Desktop;

#[tokio::test]
async fn a_blocking_call_runs_and_returns_its_value() {
    let reply = blocking(Desktop::new(), |_| DesktopResponse::ok("probe", serde_json::json!({}))).await;
    assert!(reply.ok);
}

#[tokio::test]
async fn an_observation_that_fails_comes_back_as_its_reply() {
    let failed = observe_async(
        Desktop::new(),
        "__tinydesktop_missing__".to_owned(),
        None,
        super::Depth::Full,
    )
    .await;
    assert!(failed.is_err());
}
