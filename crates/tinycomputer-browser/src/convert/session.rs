//! Starting a session: the explicit launch, the viewport, and the allowed domains.

use serde_json::{Value, json};
use tinycomputer_bus::browser::{
    Action, EvaluateRequest, LocateBy, Locator, NavigateRequest, ReadFormat, ReadRequest,
    ScreenshotRequest, ScrollDirection, SessionOptions, SnapshotRequest, Target, WaitState,
    WaitUntil,
};

use crate::error::{Error, Result};
use super::action;

/// The explicit `launch` every session starts with.
///
/// Sending it explicitly matters: without one, agent-browser auto-launches
/// from the `AGENT_BROWSER_*` environment of whatever process hosts the
/// module, which is not this session's configuration.
#[must_use]
pub(crate) fn launch(options: &SessionOptions) -> Value {
    let mut command = json!({
        "action": "launch",
        "headless": options.headless,
        "args": options.args,
    });
    let fields = [
        ("cdpUrl", options.endpoint.as_ref()),
        ("executablePath", options.executable.as_ref()),
        ("userAgent", options.user_agent.as_ref()),
        ("profile", options.user_data_dir.as_ref()),
        ("downloadPath", options.download_dir.as_ref()),
    ];
    for (key, value) in fields {
        if let Some(value) = value {
            command[key] = json!(value);
        }
    }
    let domains = allowed_domains(&options.allowed_origins);
    if !domains.is_empty() {
        command["allowedDomains"] = json!(domains);
    }
    command
}

/// The viewport a session applies straight after launching.
#[must_use]
pub(crate) fn viewport(options: &SessionOptions) -> Value {
    json!({
        "action": "viewport",
        "width": options.viewport.width,
        "height": options.viewport.height,
        "deviceScaleFactor": options.viewport.device_scale_factor,
        "mobile": options.viewport.mobile,
    })
}

/// Origins as agent-browser domain patterns.
///
/// `https://example.com` admits that host; `https://.example.com` admits it
/// and its subdomains, which agent-browser spells `*.example.com`. The
/// engine filters by host, so the scheme is not enforced — the origin
/// allow-list was a guard rail, never a sandbox, and still is.
#[must_use]
pub(crate) fn allowed_domains(origins: &[String]) -> Vec<String> {
    origins
        .iter()
        .filter_map(|origin| {
            let host = origin
                .split_once("://")
                .map_or(origin.as_str(), |(_, rest)| rest)
                .split(['/', ':'])
                .next()?
                .trim()
                .to_ascii_lowercase();
            if host.is_empty() {
                None
            } else if let Some(domain) = host.strip_prefix('.') {
                Some(format!("*.{domain}"))
            } else {
                Some(host)
            }
        })
        .collect()
}
