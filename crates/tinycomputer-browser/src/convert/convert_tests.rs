//! Tests pinning each contract request to the agent-browser command it becomes.
//!
//! These are the wire form on the engine side: a field agent-browser does not
//! read is a silent no-op at runtime, so every name is asserted here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::error::Error;

mod interaction_tests;
mod page_tests;
mod session_tests;

fn invalid(result: crate::Result<serde_json::Value>) -> String {
    match result {
        Err(Error::InvalidInput { message }) => message,
        other => panic!("expected invalid input, got {other:?}"),
    }
}
