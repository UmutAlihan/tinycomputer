//! The [`Surface`] implementation: observing, acting, reading, pasting,
//! pressing, and navigating, with the engine targets and key spellings they use.

use super::card::selects_on_click;
use super::envelope::{covered, failure, not_a_text_field, reply};
use super::sight;
use super::{NETWORK_IDLE_MS, SETTLE_MS, SKELETON_DEPTH, tree};

use serde_json::{Value, json};
use tinycomputer_bus::browser::{
    Action, NavigateRequest, ScrollDirection, SnapshotRequest, Target, WaitState,
};
use tinycomputer_bus::{DesktopError, DesktopResponse, JevOperation};
use tinycomputer_core::surface::{Candidate, Depth, Screen, Surface, uses_pointer};
use tinycomputer_core::{Key, Platform};

use super::{BrowserSurface, Perception};

impl Surface for BrowserSurface {
    fn observe(
        &self,
        app: &str,
        root: Option<&str>,
        depth: Depth,
    ) -> std::result::Result<Screen, Box<DesktopResponse>> {
        if self.perception == Perception::Sight
            && let Some(mut screen) = self.see(root)
        {
            if !app.is_empty() {
                app.clone_into(&mut screen.app);
            }
            return Ok(screen);
        }
        let request = SnapshotRequest {
            selector: root.map(sight::selector),
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
                |reference| self.perform(command, action(self::target(&reference), text.clone())),
            )
        };
        if let Some(reference) = reference
            .as_deref()
            .filter(|_| uses_pointer(operation) || operation == JevOperation::TypeText)
        {
            self.show_cursor(reference);
        }
        match operation {
            JevOperation::Click | JevOperation::Expand | JevOperation::Collapse => {
                let reply = targeted("click", |target, _| Action::Click {
                    target,
                    new_tab: false,
                });
                let name = target.as_ref().and_then(|node| node.name.as_deref());
                let reply = match (&reference, name) {
                    (Some(reference), name)
                        if covered(&reply) && (name.is_some() || sight::is_seen(reference)) =>
                    {
                        self.click_through_own_card(reference, name.unwrap_or_default())
                            .unwrap_or(reply)
                    }
                    _ => reply,
                };
                if reply.ok
                    && let (Some(reference), Some(node)) = (&reference, &target)
                    && selects_on_click(node)
                    && sight::is_seen(reference)
                {
                    self.select_if_ignored(reference);
                }
                reply
            }
            // Without a target the text goes where the focus is, as into an
            // autocomplete's unnamed input once it has been opened — but
            // only once the focused element is verified to actually take
            // typed text; a page that moved focus elsewhere (or nowhere)
            // must refuse rather than silently deliver the text to whatever
            // it finds, which could otherwise leak a private value into an
            // unrelated field.
            JevOperation::TypeText if reference.is_none() => {
                if !self.focused_field_is_editable() {
                    return DesktopResponse::err(
                        "type-text",
                        DesktopError::new("INVALID_TARGET", "no editable field has focus"),
                    );
                }
                self.perform(
                    "type-text",
                    Action::Type {
                        target: None,
                        text: text.unwrap_or_default(),
                        delay_ms: None,
                    },
                )
            }
            JevOperation::TypeText => {
                if let Some(reference) = reference.as_deref()
                    && !self.takes_text(reference)
                {
                    return not_a_text_field();
                }
                targeted("type-text", |target, text| Action::Fill {
                    target,
                    value: text.unwrap_or_default(),
                })
            }
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
                    target: reference.as_deref().map(self::target),
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
        let selector = sight::selector(&target.ref_id);
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
                target: self::target(&target.ref_id),
            },
        );
        if !focused.ok {
            return focused;
        }
        if !self.focused_field_is_editable() {
            return not_a_text_field();
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
        if let Ok(id) = self.ensure_session() {
            let _idle = self.block(self.browser.command(
                &id,
                json!({"action": "waitforloadstate", "state": "networkidle", "timeout": NETWORK_IDLE_MS}),
            ));
        }
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

    fn back(&self, _app: &str) -> DesktopResponse {
        self.perform("back", Action::Back)
    }
}

/// How the engine addresses `reference`: a ref sight minted by its mark's
/// CSS selector, a tree ref as itself.
pub(super) fn target(reference: &str) -> Target {
    if sight::is_seen(reference) {
        Target::Selector {
            value: sight::selector(reference),
        }
    } else {
        Target::reference(reference)
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
