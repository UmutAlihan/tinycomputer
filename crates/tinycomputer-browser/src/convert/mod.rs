//! Contract requests to agent-browser commands.
//!
//! agent-browser's dispatcher takes one JSON object per command, named by its
//! `action` field — the same objects its daemon receives over a socket. Every
//! function here is pure: it builds that object and nothing else, so the whole
//! mapping is testable without a browser.
//!
//! Where the contract asks for something the engine cannot express, the
//! function refuses with [`Error::InvalidInput`] naming the alternative, rather
//! than approximating it silently.

use serde_json::{Value, json};
use tinycomputer_bus::browser::{
    Action, EvaluateRequest, LocateBy, Locator, NavigateRequest, ReadFormat, ReadRequest,
    ScreenshotRequest, ScrollDirection, SessionOptions, SnapshotRequest, Target, WaitState,
    WaitUntil,
};

use crate::error::{Error, Result};

/// A distance that reaches either end of any page, for `top` and `bottom`.
const TO_THE_END: u32 = 1_000_000;

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

/// One interaction.
///
/// # Errors
///
/// [`Error::InvalidInput`] when the action names a locator the engine cannot
/// combine with it, or carries an empty argument.
pub(crate) fn action(action: &Action, default_timeout_ms: u64) -> Result<Value> {
    match action {
        Action::Scroll {
            direction,
            pixels,
            target,
        } => scroll(*direction, *pixels, target.as_ref()),
        Action::WaitFor {
            target,
            text,
            state,
            ms,
            timeout_ms,
        } => wait_for(
            target.as_ref(),
            text.as_deref(),
            *state,
            *ms,
            timeout_ms.unwrap_or(default_timeout_ms),
        ),
        Action::Back => Ok(json!({"action": "back"})),
        Action::Forward => Ok(json!({"action": "forward"})),
        Action::Reload => Ok(json!({"action": "reload"})),
        Action::Press { key } => {
            if key.trim().is_empty() {
                return Err(Error::invalid_input("press needs a key"));
            }
            Ok(json!({"action": "press", "key": key}))
        }
        Action::Type {
            target,
            text,
            delay_ms,
        } => typing(target.as_ref(), text, *delay_ms),
        element => on_element(element),
    }
}

/// An action on one element, through a semantic locator where the engine
/// offers one.
fn on_element(action: &Action) -> Result<Value> {
    Ok(match action {
        Action::Click { target, new_tab } => match target {
            Target::Locator { value } => semantic(value, "click", None)?,
            other => {
                json!({"action": "click", "selector": selector(other, "click")?, "newTab": new_tab})
            }
        },
        Action::DoubleClick { target } => {
            json!({"action": "dblclick", "selector": selector(target, "double_click")?})
        }
        Action::Hover { target } => match target {
            Target::Locator { value } => semantic(value, "hover", None)?,
            other => json!({"action": "hover", "selector": selector(other, "hover")?}),
        },
        Action::Focus { target } => {
            json!({"action": "focus", "selector": selector(target, "focus")?})
        }
        Action::Fill { target, value } => match target {
            Target::Locator { value: locator } => semantic(locator, "fill", Some(value))?,
            other => {
                json!({"action": "fill", "selector": selector(other, "fill")?, "value": value})
            }
        },
        Action::Select { target, values } => {
            json!({"action": "select", "selector": selector(target, "select")?, "values": values})
        }
        Action::Check { target, checked } => match (target, checked) {
            (Target::Locator { value }, true) => semantic(value, "check", None)?,
            (other, true) => json!({"action": "check", "selector": selector(other, "check")?}),
            (other, false) => {
                json!({"action": "uncheck", "selector": selector(other, "uncheck")?})
            }
        },
        Action::GetText { target } => match target {
            Target::Locator { value } => semantic(value, "text", None)?,
            other => json!({"action": "gettext", "selector": selector(other, "get_text")?}),
        },
        Action::GetAttribute { target, attribute } => json!({
            "action": "getattribute",
            "selector": selector(target, "get_attribute")?,
            "attribute": attribute,
        }),
        Action::IsVisible { target } => {
            json!({"action": "isvisible", "selector": selector(target, "is_visible")?})
        }
        Action::Scroll { .. }
        | Action::WaitFor { .. }
        | Action::Back
        | Action::Forward
        | Action::Reload
        | Action::Press { .. }
        | Action::Type { .. } => {
            return Err(Error::failed("not an element action"));
        }
    })
}

fn typing(target: Option<&Target>, text: &str, delay_ms: Option<u64>) -> Result<Value> {
    let Some(target) = target else {
        return Ok(json!({"action": "inserttext", "text": text}));
    };
    let mut command =
        json!({"action": "type", "selector": selector(target, "type")?, "text": text});
    if let Some(delay) = delay_ms {
        command["delay"] = json!(delay);
    }
    Ok(command)
}

fn scroll(
    direction: ScrollDirection,
    pixels: Option<u32>,
    target: Option<&Target>,
) -> Result<Value> {
    let (direction, amount) = match direction {
        ScrollDirection::Down => ("down", pixels.unwrap_or(300)),
        ScrollDirection::Up => ("up", pixels.unwrap_or(300)),
        ScrollDirection::Left => ("left", pixels.unwrap_or(300)),
        ScrollDirection::Right => ("right", pixels.unwrap_or(300)),
        ScrollDirection::Top => ("up", TO_THE_END),
        ScrollDirection::Bottom => ("down", TO_THE_END),
    };
    let mut command = json!({"action": "scroll", "direction": direction, "amount": amount});
    if let Some(target) = target {
        command["selector"] = json!(selector(target, "scroll")?);
    }
    Ok(command)
}

fn wait_for(
    target: Option<&Target>,
    text: Option<&str>,
    state: WaitState,
    ms: Option<u64>,
    timeout: u64,
) -> Result<Value> {
    match (text, target, ms) {
        (Some(text), _, _) => Ok(json!({"action": "wait", "text": text, "timeout": timeout})),
        (None, Some(target), _) => Ok(json!({
            "action": "wait",
            "selector": selector(target, "wait_for")?,
            "state": wait_state(state),
            "timeout": timeout,
        })),
        (None, None, Some(ms)) => Ok(json!({"action": "wait", "timeout": ms})),
        (None, None, None) => Err(Error::invalid_input("wait_for needs a target, text, or ms")),
    }
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

/// The engine selector for a ref or CSS target.
///
/// # Errors
///
/// [`Error::InvalidInput`] for a locator, which agent-browser only accepts on
/// its semantic commands (click, fill, check, hover, text).
fn selector(target: &Target, operation: &str) -> Result<String> {
    match target {
        Target::Ref { value } if !value.trim().is_empty() => {
            Ok(format!("@{}", value.trim_start_matches('@')))
        }
        Target::Selector { value } if !value.trim().is_empty() => Ok(value.clone()),
        Target::Ref { .. } | Target::Selector { .. } => Err(Error::invalid_input(format!(
            "{operation} needs a non-empty target"
        ))),
        Target::Locator { .. } => Err(Error::invalid_input(format!(
            "{operation} takes a ref or selector; locators work with click, fill, check, hover, and get_text"
        ))),
    }
}

/// A semantic-locator command performing `subaction` on the match.
///
/// # Errors
///
/// [`Error::InvalidInput`] for an empty value or a non-first match index.
fn semantic(locator: &Locator, subaction: &str, value: Option<&String>) -> Result<Value> {
    if locator.value.trim().is_empty() {
        return Err(Error::invalid_input("a locator needs a value"));
    }
    if locator.index != 0 {
        return Err(Error::invalid_input(
            "locator index is not supported; take a snapshot and use a ref",
        ));
    }
    let (action, key) = match locator.by {
        LocateBy::Role => ("getbyrole", "role"),
        LocateBy::Text => ("getbytext", "text"),
        LocateBy::Label => ("getbylabel", "label"),
        LocateBy::Placeholder => ("getbyplaceholder", "placeholder"),
        LocateBy::TestId => ("getbytestid", "testId"),
        LocateBy::AltText => ("getbyalttext", "text"),
        LocateBy::Title => ("getbytitle", "text"),
    };
    let mut command = json!({
        "action": action,
        key: locator.value,
        "exact": locator.exact,
        "subaction": subaction,
    });
    if let Some(name) = &locator.name {
        command["name"] = json!(name);
    }
    if let Some(value) = value {
        command["value"] = json!(value);
    }
    Ok(command)
}

fn wait_state(state: WaitState) -> &'static str {
    match state {
        WaitState::Attached => "attached",
        WaitState::Detached => "detached",
        WaitState::Visible => "visible",
        WaitState::Hidden => "hidden",
    }
}

#[cfg(test)]
mod test;
