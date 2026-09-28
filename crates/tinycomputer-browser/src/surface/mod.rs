//! [`BrowserSurface`]: one browser session as a decision loop's
//! [`Surface`], so the flow runtime drives a web page the way it drives a
//! desktop application.
//!
//! Surface calls block, and the flow runtime makes them off its executor
//! (`spawn_blocking`); each one here runs the async [`Browser`] call to
//! completion on the runtime handle the surface was built with. The session
//! opens lazily, on the first call that needs a page.
//!
//! When the session has a window on screen, the agent's cursor glides onto
//! each element before the surface acts on it (`cursor.rs`). It is cosmetic:
//! the actions are the same with or without it.

mod cursor;
mod sight;
mod tree;

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tinycomputer_bus::browser::{
    Action, NavigateRequest, ScrollDirection, SessionId, SessionOptions, SnapshotRequest, Target,
    WaitState,
};
use tinycomputer_bus::{DesktopError, DesktopResponse, JevOperation};
use tinycomputer_core::surface::{Candidate, Depth, Screen, Surface, uses_pointer};
use tinycomputer_core::{Key, Platform};
use tinycomputer_cursor::ScreenCursor;

use crate::error::{Error, Result};
use crate::sessions::Browser;

/// How deep a skeleton observation reads before a flow drills in.
const SKELETON_DEPTH: u32 = 6;

/// How long the page is given to react once its network is quiet: long
/// enough for a banner or menu to finish closing.
const SETTLE_MS: u64 = 400;

/// The longest a settle waits for the page's network to go quiet; a page
/// that polls forever is never idle, so this is a cap, not an expectation.
const NETWORK_IDLE_MS: u64 = 2_000;

/// Whether what covers a point belongs to the same result card as the
/// element that was meant, so the click may go through it. Many result lists
/// lay a transparent click layer, or the card's own text, over each card's
/// link, so the card's own controls are always "covered" — by the card
/// itself.
///
/// The target is found by its name, *exactly*: among the elements stacked at
/// the point (`elementsFromPoint`, topmost first), or, when the card's
/// content sits over it — Google Flights puts each card's duration text
/// above its "Select flight" link — as the one element on the page whose
/// `aria-label` is that name and whose box holds the point (Google renders
/// each flight twice, once in a hidden tab). Whitespace runs count as one
/// space, as they do in an accessible name. A short name such as "Select"
/// appears in almost any card, so containment is never enough, and a label
/// two elements at the point share matches neither. A ref sight minted
/// passes its mark's selector as `exact`, and is the target itself, name or
/// no name. The click goes through only when what is
/// on top sits inside the target's own card (`li`, `listitem`, `row`,
/// `article`) and inside no dialog; a banner or dialog in front still
/// blocks it.
const SAME_CARD_JS: &str = r#"((x, y, name, exact) => {
  if (!name && !exact) return false;
  const stack = document.elementsFromPoint(x, y);
  const top = stack[0];
  if (!top) return false;
  const squash = (text) => text.replace(/\s+/g, ' ').trim();
  name = squash(name || '');
  const shown = (element) => squash(element.getAttribute('aria-label') || element.innerText || '');
  const under = (element) => {
    const box = element.getBoundingClientRect();
    return box.width > 0 && box.height > 0
      && x >= box.left && x <= box.right && y >= box.top && y <= box.bottom;
  };
  const labelled = [...document.querySelectorAll('[aria-label]')]
    .filter((element) => squash(element.getAttribute('aria-label')) === name && under(element));
  const marked = exact ? document.querySelector(exact) : null;
  const target = exact ? (marked && under(marked) ? marked : null)
    : stack.find((element) => shown(element) === name)
      || (labelled.length === 1 ? labelled[0] : null);
  if (!target || target === top) return false;
  const card = target.closest('li,[role="listitem"],[role="row"],article,[role="article"]');
  const modal = top.closest('dialog,[role="dialog"],[role="alertdialog"],[aria-modal="true"]');
  return Boolean(card && card.contains(top) && !modal);
})"#;

/// How a [`BrowserSurface`] reads a page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Perception {
    /// As a person looks at it: what is drawn and on top, the words on and
    /// beside each control, and which boxes take text, read from the
    /// rendered page. Falls back to the accessibility tree when it cannot
    /// reach what it sees (a shadow root, a frame in front) or the reading
    /// fails.
    #[default]
    Sight,
    /// Through the accessibility tree alone: roles and names as the page's
    /// markup declares them.
    Tree,
}

/// One browser session, lazily opened, as a [`Surface`].
#[derive(Clone)]
pub struct BrowserSurface {
    browser: Arc<Browser>,
    options: SessionOptions,
    session: Arc<Mutex<Option<SessionId>>>,
    handle: tokio::runtime::Handle,
    platform: Platform,
    cursor: Arc<ScreenCursor>,
    perception: Perception,
}

impl std::fmt::Debug for BrowserSurface {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BrowserSurface")
            .field("session", &self.session)
            .field("platform", &self.platform)
            .field("cursor", &self.cursor)
            .field("perception", &self.perception)
            .finish_non_exhaustive()
    }
}

impl BrowserSurface {
    /// A surface that opens its session on `browser` with `options`, and
    /// runs browser calls on `handle`. It draws no cursor until given one
    /// with [`BrowserSurface::with_cursor`].
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
            cursor: Arc::new(ScreenCursor::off()),
            perception: Perception::default(),
        }
    }

    /// The same surface, reading pages with `perception`
    /// ([`Perception::Sight`] unless told otherwise).
    #[must_use]
    pub fn with_perception(mut self, perception: Perception) -> Self {
        self.perception = perception;
        self
    }

    /// The same surface, drawing on `cursor` — the screen's one agent
    /// cursor, shared with the desktop surface — whenever its session has a
    /// window on screen. The cursor is cosmetic: every action is performed
    /// the same way with or without it.
    #[must_use]
    pub fn with_cursor(mut self, cursor: Arc<ScreenCursor>) -> Self {
        self.cursor = cursor;
        self
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

    /// Clicks the middle of `reference` even though something covers it,
    /// but only when the cover is part of the same result card, so a banner
    /// or dialog in front still blocks the click. `None` when it is not.
    fn click_through_own_card(&self, reference: &str, name: &str) -> Option<DesktopResponse> {
        let id = self.ensure_session().ok()?;
        let selector = sight::selector(reference);
        let bounds = self
            .block(
                self.browser
                    .command(&id, json!({"action": "boundingbox", "selector": selector})),
            )
            .ok()?;
        let middle = |start: &str, size: &str| {
            Some(bounds.get(start)?.as_f64()? + bounds.get(size)?.as_f64()? / 2.0)
        };
        let (x, y) = (middle("x", "width")?, middle("y", "height")?);
        // The JS side now requires an exact match against the target's own
        // shown text, so the name is passed through untruncated: cutting it
        // short would make an exact match against the page's full text
        // impossible for any control with a longer name.
        let name = name.trim();
        // A ref sight minted names its element exactly, by its mark; a
        // tree ref is found by its name.
        let exact = if sight::is_seen(reference) {
            serde_json::to_string(&selector).ok()?
        } else {
            "null".to_owned()
        };
        let script = format!(
            "{SAME_CARD_JS}({x}, {y}, {}, {exact})",
            serde_json::to_string(&name).ok()?
        );
        let same_card = self
            .block(
                self.browser
                    .command(&id, json!({"action": "evaluate", "script": script})),
            )
            .ok()?;
        if same_card.get("result") != Some(&Value::Bool(true)) {
            return None;
        }
        for event in ["mouseMoved", "mousePressed", "mouseReleased"] {
            let pressed = event != "mouseMoved";
            self.block(self.browser.command(
                &id,
                json!({
                    "action": "mouse",
                    "eventType": event,
                    "x": x,
                    "y": y,
                    "button": if pressed { "left" } else { "none" },
                    "clickCount": i32::from(pressed),
                }),
            ))
            .ok()?;
        }
        Some(DesktopResponse::ok(
            "click",
            json!({"clicked": selector, "through": "its own card's click layer"}),
        ))
    }

    /// Whether the page's currently focused element takes typed text: a
    /// text-like `<input>` that is neither read-only nor disabled, a
    /// `<textarea>`, or a `contenteditable` region. An ARIA role alone does
    /// not count: pages give `combobox` and `textbox` roles to list rows and
    /// buttons that hold no text (measured on a booking widget, whose city
    /// rows are `div role="combobox"`). Typing without a target sends keys
    /// wherever the browser's own focus happens to be, so this is checked
    /// first: a stale or unexpected focus — an unrelated field, or none at
    /// all — must never silently receive text, including a private value.
    fn focused_field_is_editable(&self) -> bool {
        const SCRIPT: &str = r"(() => {
  const element = document.activeElement;
  if (!element) return false;
  const tag = (element.tagName || '').toLowerCase();
  if (tag === 'textarea' || element.isContentEditable) return true;
  if (tag !== 'input') return false;
  const type = (element.getAttribute('type') || 'text').toLowerCase();
  return ['text', 'search', 'email', 'tel', 'url', 'number', 'password'].includes(type)
    && !element.readOnly && !element.disabled;
})()";
        let Ok(id) = self.ensure_session() else {
            return false;
        };
        self.block(
            self.browser
                .command(&id, json!({"action": "evaluate", "script": SCRIPT})),
        )
        .ok()
        .and_then(|data| data.get("result").and_then(Value::as_bool))
        .unwrap_or(false)
    }

    /// Whether the element `reference` names takes typed text: focusing it
    /// lands on an input, a text area, an editable region, or a text-box
    /// role. A page can give any `div` a `combobox` or `textbox` role — a
    /// city in a list of suggestions, a card — and a fill or a paste into
    /// one reports success while nothing holds the text.
    fn takes_text(&self, reference: &str) -> bool {
        self.perform(
            "focus",
            Action::Focus {
                target: target(reference),
            },
        )
        .ok && self.focused_field_is_editable()
    }

    /// The page, or the part of it under `root`, read by sight; `None` when
    /// the reading fails or sees what it cannot reach, and the tree is read
    /// instead.
    fn see(&self, root: Option<&str>) -> Option<Screen> {
        let id = self.ensure_session().ok()?;
        let reply = self
            .block(self.browser.command(
                &id,
                json!({"action": "evaluate", "script": sight::script(root)}),
            ))
            .ok()?;
        sight::screen(reply.get("result")?)
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
                |reference| {
                    self.perform(command, action(target(&reference), text.clone()))
                },
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
                match (&reference, name) {
                    (Some(reference), name)
                        if covered(&reply) && (name.is_some() || sight::is_seen(reference)) =>
                    {
                        self.click_through_own_card(reference, name.unwrap_or_default())
                            .unwrap_or(reply)
                    }
                    _ => reply,
                }
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
                    target: reference.as_deref().map(target),
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
}

/// How the engine addresses `reference`: a ref sight minted by its mark's
/// CSS selector, a tree ref as itself.
fn target(reference: &str) -> Target {
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

/// A browser result as the envelope decision loops read.
fn reply(command: &str, result: Result<Value>) -> DesktopResponse {
    match result {
        Ok(data) => DesktopResponse::ok(command, data),
        Err(error) => failure(command, &error),
    }
}

/// A browser error as an envelope failure whose code is the error's wire
/// name in `SCREAMING_SNAKE_CASE`, such as `STALE_REF`.
/// Whether a click was refused because another element covers its target.
fn covered(reply: &DesktopResponse) -> bool {
    reply
        .error
        .as_ref()
        .is_some_and(|error| error.message.contains("is covered by"))
}

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

/// The refusal for text aimed at an element that does not take it.
fn not_a_text_field() -> DesktopResponse {
    DesktopResponse::err(
        "type-text",
        DesktopError::new(
            "NOT_A_TEXT_FIELD",
            "the element does not take typed text: focusing it reaches no input, text area, or editable region",
        ),
    )
}

#[cfg(test)]
mod test;
