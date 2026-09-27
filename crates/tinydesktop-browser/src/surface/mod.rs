//! [`BrowserSurface`]: one browser session as a decision loop's
//! [`Surface`], so the flow runtime drives a web page the way it drives a
//! desktop application.
//!
//! Surface calls block, and the flow runtime makes them off its executor
//! (`spawn_blocking`); each one here runs the async [`Browser`] call to
//! completion on the runtime handle the surface was built with. The session
//! opens lazily, on the first call that needs a page.

mod tree;

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tinydesktop_bus::browser::{
    Action, NavigateRequest, ScrollDirection, SessionId, SessionOptions, SnapshotRequest, Target,
    WaitState,
};
use tinydesktop_bus::{DesktopError, DesktopResponse, JevOperation};
use tinydesktop_core::surface::{Candidate, Depth, Screen, Surface};
use tinydesktop_core::{Key, Platform};

use crate::error::{Error, Result};
use crate::sessions::Browser;

/// How deep a skeleton observation reads before a flow drills in.
const SKELETON_DEPTH: u32 = 6;

/// How long the page is given to react before a value is read back again.
const SETTLE_MS: u64 = 200;

/// One browser session, lazily opened, as a [`Surface`].
#[derive(Clone)]
pub struct BrowserSurface {
    browser: Arc<Browser>,
    options: SessionOptions,
    session: Arc<Mutex<Option<SessionId>>>,
    handle: tokio::runtime::Handle,
    platform: Platform,
}

impl std::fmt::Debug for BrowserSurface {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BrowserSurface")
            .field("session", &self.session)
            .field("platform", &self.platform)
            .finish_non_exhaustive()
    }
}

impl BrowserSurface {
    /// A surface that opens its session on `browser` with `options`, and
    /// runs browser calls on `handle`.
    #[must_use]
    pub fn new(
        browser: Arc<Browser>,
        options: SessionOptions,
        handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            browser,
            options,
            session: Arc::new(Mutex::new(None)),
            handle,
            platform: Platform::current(),
        }
    }

    /// The session this surface drives, once one is open.
    #[must_use]
    pub fn session(&self) -> Option<SessionId> {
        self.session.lock().ok().and_then(|session| session.clone())
    }

    /// Closes the session, if one is open, without waiting for it: safe to
    /// call from async code, where a blocking surface call is not.
    pub fn close(&self) {
        let Some(id) = self
            .session
            .lock()
            .ok()
            .and_then(|mut session| session.take())
        else {
            return;
        };
        let browser = self.browser.clone();
        self.handle.spawn(async move {
            let _closed = browser.close_session(&id).await;
        });
    }

    fn block<T>(&self, future: impl std::future::Future<Output = T>) -> T {
        self.handle.block_on(future)
    }

    fn ensure_session(&self) -> Result<SessionId> {
        let mut slot = self
            .session
            .lock()
            .map_err(|_| Error::failed("the browser surface was poisoned by a panic"))?;
        if let Some(id) = slot.as_ref() {
            return Ok(id.clone());
        }
        let info = self.block(self.browser.open_session(self.options.clone()))?;
        *slot = Some(info.id.clone());
        Ok(info.id)
    }

    fn perform(&self, command: &str, action: Action) -> DesktopResponse {
        let outcome = self.ensure_session().and_then(|id| {
            self.block(self.browser.perform(&id, action))
                .map(|outcome| json!({"value": outcome.value, "url": outcome.page.url}))
        });
        reply(command, outcome)
    }
}

impl Surface for BrowserSurface {
    fn observe(
        &self,
        app: &str,
        root: Option<&str>,
        depth: Depth,
    ) -> std::result::Result<Screen, Box<DesktopResponse>> {
        let request = SnapshotRequest {
            selector: root.map(|reference| format!("@{}", reference.trim_start_matches('@'))),
            depth: (depth == Depth::Skeleton && root.is_none()).then_some(SKELETON_DEPTH),
            ..SnapshotRequest::default()
        };
        let snapshot = self
            .ensure_session()
            .and_then(|id| self.block(self.browser.snapshot(&id, request)))
            .map_err(|error| Box::new(failure("snapshot", &error)))?;
        let mut screen = tree::screen(&snapshot.tree, &snapshot.title);
        if !app.is_empty() {
            app.clone_into(&mut screen.app);
        }
        Ok(screen)
    }

    fn execute(
        &self,
        operation: JevOperation,
        target: Option<Candidate>,
        text: Option<String>,
    ) -> DesktopResponse {
        let reference = target
            .as_ref()
            .map(|node| node.ref_id.clone())
            .filter(|reference| !reference.is_empty());
        let targeted = |command: &str, action: fn(Target, Option<String>) -> Action| {
            reference.clone().map_or_else(
                || {
                    DesktopResponse::err(
                        command,
                        DesktopError::new("INVALID_TARGET", "the operation needs a target"),
                    )
                },
                |reference| {
                    self.perform(command, action(Target::reference(reference), text.clone()))
                },
            )
        };
        match operation {
            JevOperation::Click | JevOperation::Expand | JevOperation::Collapse => {
                targeted("click", |target, _| Action::Click {
                    target,
                    new_tab: false,
                })
            }
            JevOperation::TypeText => targeted("type-text", |target, text| Action::Fill {
                target,
                value: text.unwrap_or_default(),
            }),
            JevOperation::Check => targeted("check", |target, _| Action::Check {
                target,
                checked: true,
            }),
            JevOperation::Uncheck => targeted("uncheck", |target, _| Action::Check {
                target,
                checked: false,
            }),
            JevOperation::Scroll => self.perform(
                "scroll",
                Action::Scroll {
                    direction: ScrollDirection::Down,
                    pixels: None,
                    target: reference.map(Target::reference),
                },
            ),
            JevOperation::Wait => self.perform("wait", pause(500)),
            JevOperation::Drill | JevOperation::Widen => {
                DesktopResponse::ok("look", json!({"root": reference}))
            }
            JevOperation::Done | JevOperation::Blocked => {
                DesktopResponse::ok("resolve-intent", json!({}))
            }
        }
    }

    fn read_value(&self, target: &Candidate) -> Option<String> {
        if target.ref_id.is_empty() {
            return None;
        }
        let id = self.ensure_session().ok()?;
        let selector = format!("@{}", target.ref_id);
        ["inputvalue", "gettext"].into_iter().find_map(|action| {
            let data = self
                .block(
                    self.browser
                        .command(&id, json!({"action": action, "selector": selector})),
                )
                .ok()?;
            data.get("value")
                .or_else(|| data.get("text"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
        })
    }

    /// Types at the field's caret: focus it, select what it holds when it is
    /// a plain field, and insert the text. A page needs no clipboard for this.
    fn paste(&self, app: &str, target: &Candidate, text: &str) -> DesktopResponse {
        if target.ref_id.is_empty() {
            return DesktopResponse::err(
                "paste",
                DesktopError::new("INVALID_TARGET", "paste needs a target"),
            );
        }
        let focused = self.perform(
            "focus",
            Action::Focus {
                target: Target::reference(&target.ref_id),
            },
        );
        if !focused.ok {
            return focused;
        }
        if target
            .available_actions
            .iter()
            .any(|action| action == "SetValue")
        {
            let selected = self.press(app, "cmd+a");
            if !selected.ok {
                return selected;
            }
        }
        self.perform(
            "paste",
            Action::Type {
                target: None,
                text: text.to_owned(),
                delay_ms: None,
            },
        )
    }

    fn press(&self, _app: &str, combo: &str) -> DesktopResponse {
        let key = browser_key(combo, self.platform);
        if key.is_empty() {
            return DesktopResponse::err(
                "press",
                DesktopError::new("INVALID_KEY", "press needs a key"),
            );
        }
        self.perform("press", Action::Press { key })
    }

    fn launch(&self, _app: &str) -> DesktopResponse {
        reply(
            "launch",
            self.ensure_session()
                .map(|id| json!({"running": true, "session": id})),
        )
    }

    fn settle(&self) {
        let _settled = self.perform("wait", pause(SETTLE_MS));
    }

    fn navigate(&self, url: &str) -> DesktopResponse {
        let page = self
            .ensure_session()
            .and_then(|id| self.block(self.browser.navigate(&id, NavigateRequest::new(url))));
        reply(
            "navigate",
            page.map(|page| json!({"url": page.url, "title": page.title})),
        )
    }
}

fn pause(ms: u64) -> Action {
    Action::WaitFor {
        target: None,
        text: None,
        state: WaitState::Visible,
        ms: Some(ms),
        timeout_ms: None,
    }
}

/// A flow's key combination (`cmd+a`, `return`) as agent-browser spells it
/// (`Meta+a`, `Enter`). The logical `cmd` becomes the platform's command key.
#[must_use]
pub(crate) fn browser_key(combo: &str, platform: Platform) -> String {
    let command = Key::SelectAll
        .browser(platform)
        .and_then(|select_all| select_all.split('+').next().map(str::to_owned))
        .unwrap_or_else(|| "Control".to_owned());
    combo
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| match part.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "meta" => command.clone(),
            "ctrl" | "control" => "Control".to_owned(),
            "shift" => "Shift".to_owned(),
            "alt" | "option" => "Alt".to_owned(),
            "return" | "enter" => "Enter".to_owned(),
            "escape" | "esc" => "Escape".to_owned(),
            "tab" => "Tab".to_owned(),
            "space" => "Space".to_owned(),
            "backspace" => "Backspace".to_owned(),
            "delete" => "Delete".to_owned(),
            "left" | "right" | "up" | "down" => {
                let mut arrow = "Arrow".to_owned();
                let mut direction = part.to_ascii_lowercase();
                direction[..1].make_ascii_uppercase();
                arrow.push_str(&direction);
                arrow
            }
            other if other.len() == 1 => other.to_owned(),
            other => {
                let mut named = other.to_owned();
                named[..1].make_ascii_uppercase();
                named
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// A browser result as the envelope decision loops read.
fn reply(command: &str, result: Result<Value>) -> DesktopResponse {
    match result {
        Ok(data) => DesktopResponse::ok(command, data),
        Err(error) => failure(command, &error),
    }
}

/// A browser error as an envelope failure whose code is the error's wire
/// name in `SCREAMING_SNAKE_CASE`, such as `STALE_REF`.
fn failure(command: &str, error: &Error) -> DesktopResponse {
    let name = error
        .wire_name()
        .rsplit('.')
        .next()
        .unwrap_or("ModuleFailed");
    let mut code = String::with_capacity(name.len() + 4);
    for (index, character) in name.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            code.push('_');
        }
        code.push(character.to_ascii_uppercase());
    }
    DesktopResponse::err(command, DesktopError::new(code, error.to_string()))
}

#[cfg(test)]
mod test;
