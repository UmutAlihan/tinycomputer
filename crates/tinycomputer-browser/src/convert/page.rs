//! Whole-page commands: navigation, snapshots, extraction, evaluation, and
//! screenshots.

use serde_json::{Value, json};
use tinycomputer_bus::browser::{
    EvaluateRequest, NavigateRequest, ReadFormat, ReadRequest, ScreenshotRequest, SnapshotRequest,
    WaitUntil,
};

use super::interaction::selector;
use crate::error::{Error, Result};

/// A navigation.
///
/// # Errors
///
/// [`Error::InvalidInput`] when the URL is empty.
pub(crate) fn navigate(request: &NavigateRequest) -> Result<Value> {
    if request.url.trim().is_empty() {
        return Err(Error::invalid_input("navigate needs a url"));
    }
    let wait_until = match request.wait_until {
        WaitUntil::Commit => "none",
        WaitUntil::DomContentLoaded => "domcontentloaded",
        WaitUntil::Load => "load",
        WaitUntil::NetworkIdle => "networkidle",
    };
    let mut command = json!({"action": "navigate", "url": request.url, "waitUntil": wait_until});
    if let Some(timeout) = request.timeout_ms {
        command["timeout"] = json!(timeout);
    }
    Ok(command)
}

/// An accessibility snapshot.
#[must_use]
pub(crate) fn snapshot(request: &SnapshotRequest) -> Value {
    let mut command = json!({
        "action": "snapshot",
        "interactive": request.interactive_only,
        "compact": request.compact,
        "urls": request.include_urls,
    });
    if let Some(depth) = request.depth {
        command["maxDepth"] = json!(depth);
    }
    if let Some(selector) = &request.selector {
        command["selector"] = json!(selector);
    }
    command
}

/// Page extraction, in the requested format.
#[must_use]
pub(crate) fn read(request: &ReadRequest) -> Value {
    match (request.format, &request.selector) {
        (ReadFormat::Markdown, None) => json!({"action": "read"}),
        (ReadFormat::Html, None) => json!({"action": "content"}),
        (ReadFormat::Html, Some(selector)) => {
            json!({"action": "innerhtml", "selector": selector})
        }
        (ReadFormat::Text | ReadFormat::Markdown, Some(selector)) => {
            json!({"action": "gettext", "selector": selector})
        }
        (ReadFormat::Text, None) => json!({"action": "gettext", "selector": "body"}),
    }
}

/// A script evaluation.
///
/// # Errors
///
/// [`Error::InvalidInput`] when the expression is empty.
pub(crate) fn evaluate(request: &EvaluateRequest) -> Result<Value> {
    if request.expression.trim().is_empty() {
        return Err(Error::invalid_input("evaluate needs an expression"));
    }
    Ok(json!({"action": "evaluate", "script": request.expression}))
}

/// A screenshot written to `path`, which the session collects and deletes.
///
/// # Errors
///
/// [`Error::InvalidInput`] when the quality is outside 1–100, or the target
/// is a locator (screenshots need a ref or selector).
pub(crate) fn screenshot(request: &ScreenshotRequest, path: &str) -> Result<Value> {
    let format = match request.format {
        tinycomputer_bus::browser::ImageFormat::Png => "png",
        tinycomputer_bus::browser::ImageFormat::Jpeg => "jpeg",
        tinycomputer_bus::browser::ImageFormat::Webp => "webp",
    };
    let mut command = json!({
        "action": "screenshot",
        "path": path,
        "fullPage": request.full_page,
        "format": format,
    });
    if let Some(quality) = request.quality {
        if !(1..=100).contains(&quality) {
            return Err(Error::invalid_input("screenshot quality must be 1 to 100"));
        }
        command["quality"] = json!(quality);
    }
    if let Some(target) = &request.target {
        command["selector"] = json!(selector(target, "screenshot")?);
    }
    Ok(command)
}
