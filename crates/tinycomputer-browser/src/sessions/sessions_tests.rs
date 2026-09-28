//! Tests for sessions over a scripted engine.
//!
//! The fake answers each command the way agent-browser does, records what it
//! was sent, and writes screenshot and download files where asked, so the
//! whole session layer runs without a browser.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};
use tinycomputer_bus::browser::{
    Action, DownloadWaitRequest, EvaluateRequest, LocateBy, Locator, NavigateRequest, ReadFormat,
    ReadRequest, ScreenshotRequest, SessionId, SessionOptions, SnapshotRequest, Target,
};

use super::{Browser, MAX_SESSIONS};
use crate::error::Error;
use crate::fake::{Fake, failure, ok};

mod artifacts_tests;
mod lifecycle_tests;
mod page_tests;

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "tinycomputer-browser-test-{}-{name}",
        std::process::id()
    ))
}

async fn open(fake: &Fake, name: &str) -> (Browser, SessionId) {
    let browser = Browser::with_scratch(Arc::new(fake.clone()), scratch(name));
    let info = browser
        .open_session(SessionOptions::default())
        .await
        .unwrap();
    (browser, info.id)
}
