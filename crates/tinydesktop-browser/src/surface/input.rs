//! The virtual mouse and keyboard on a page: plans from `tinydesktop-input`
//! played as agent-browser pointer and key commands.
//!
//! Every move is a real `mouseMoved` event through CDP, so a page sees the
//! pointer arrive — `pointerover`, `mouseenter`, hover styles, menus that
//! open on hover — before anything is pressed. None of it touches the
//! operating system's pointer: the page's pointer is a second, virtual one.

use serde_json::{Value, json};
use tinydesktop_bus::browser::SessionId;
use tinydesktop_input::{Button, InputSink, Key, Plan, Point, Rect, play};

use super::BrowserSurface;
use crate::error::Result;

/// A session's pointer and keyboard as an [`InputSink`].
pub(super) struct PageInput<'a> {
    pub(super) surface: &'a BrowserSurface,
    pub(super) id: SessionId,
}

impl PageInput<'_> {
    fn send(&self, command: Value) -> Result<()> {
        self.surface
            .block(self.surface.browser.command(&self.id, command))
            .map(|_| ())
    }
}

impl InputSink for PageInput<'_> {
    type Error = crate::Error;

    fn move_to(&mut self, point: Point) -> Result<()> {
        // `instant` makes the engine dispatch exactly this one move; the
        // path, and its pacing, come from the plan.
        self.send(json!({
            "action": "mousemove",
            "x": point.x,
            "y": point.y,
            "inputMode": "instant",
        }))
    }

    fn press(&mut self, button: Button) -> Result<()> {
        self.send(json!({"action": "mousedown", "button": button.as_str()}))
    }

    fn release(&mut self, button: Button) -> Result<()> {
        self.send(json!({"action": "mouseup", "button": button.as_str()}))
    }

    fn key_down(&mut self, key: Key) -> Result<()> {
        match key {
            // A bare key event carries no virtual key code, which Enter and
            // Tab need for their default actions (submit, move focus); the
            // engine's `press` sends a complete one.
            Key::Enter | Key::Tab => self.send(json!({"action": "press", "key": key.name()})),
            Key::Char(_) => self.send(json!({
                "action": "keyboard",
                "eventType": "keyDown",
                "key": key.name(),
                "text": key.text(),
            })),
        }
    }

    fn key_up(&mut self, key: Key) -> Result<()> {
        match key {
            Key::Enter | Key::Tab => Ok(()),
            Key::Char(_) => self.send(json!({
                "action": "keyboard",
                "eventType": "keyUp",
                "key": key.name(),
            })),
        }
    }

    fn text(&mut self, text: &str) -> Result<()> {
        self.send(json!({"action": "inserttext", "text": text}))
    }
}

impl BrowserSurface {
    /// Plays `plan` on the session's page.
    pub(super) fn play(&self, plan: &Plan) -> Result<()> {
        let id = self.ensure_session()?;
        let mut sink = PageInput { surface: self, id };
        play(plan, &mut sink, &mut |duration| (self.wait)(duration))
    }

    /// Where `reference` is on the page, in viewport pixels.
    pub(super) fn bounds(&self, reference: &str) -> Option<Rect> {
        let id = self.ensure_session().ok()?;
        let selector = format!("@{}", reference.trim_start_matches('@'));
        let data = self
            .block(
                self.browser
                    .command(&id, json!({"action": "boundingbox", "selector": selector})),
            )
            .ok()?;
        let field = |name: &str| data.get(name).and_then(Value::as_f64);
        let rect = Rect::new(field("x")?, field("y")?, field("width")?, field("height")?);
        (rect.is_valid() && rect.width > 0.0 && rect.height > 0.0).then_some(rect)
    }

    /// Glides the virtual pointer onto `reference` and settles there, so the
    /// element is hovered before whatever acts on it. Best effort: an element
    /// with no box, or a move the page refuses, leaves the action to proceed
    /// as it would have without the approach.
    pub(super) fn approach(&self, reference: &str) {
        if self.motion.is_instant() {
            return;
        }
        let Some(rect) = self.bounds(reference) else {
            return;
        };
        let Ok(mut mouse) = self.mouse.lock() else {
            return;
        };
        let (plan, _) = mouse.approach(rect);
        if self.play(&plan).is_err() {
            mouse.forget();
        }
    }

    /// Clicks at `point` with the virtual pointer: reach, settle, press,
    /// release — each a real pointer event on the page.
    pub(super) fn click_at(&self, point: Point) -> Result<()> {
        let plan = self.mouse.lock().map_or_else(
            |_| Plan::new(),
            |mut mouse| mouse.click(Rect::at(point), Button::Left, 1),
        );
        self.play(&plan)
    }

    /// Types `text` key by key at the keyboard's cadence, where the focus is.
    pub(super) fn type_keys(&self, text: &str) -> Result<()> {
        let plan = self
            .keyboard
            .lock()
            .map_or_else(|_| Plan::new(), |mut keyboard| keyboard.type_text(text));
        self.play(&plan)
    }
}
