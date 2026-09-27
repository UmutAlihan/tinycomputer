//! The real pointer, gliding like a hand before a pointer action.
//!
//! Only a headed [`Desktop`] moves the pointer. It reaches the target's
//! bounds along a `tinydesktop-input` path — each sample a real `mouse_move`
//! — and settles, so the application sees the pointer arrive and hover
//! before the accessibility action lands. The glide is best effort: when it
//! cannot run, the action proceeds exactly as it would have without it.

use serde_json::Value;
use tinydesktop_bus::{DesktopResponse, MouseMoveRequest};
use tinydesktop_core::surface::Candidate;
use tinydesktop_input::{Button, InputSink, Key, Pacer, Plan, Point, Rect, play};

use crate::Desktop;

/// A [`Desktop`]'s real pointer as an [`InputSink`] that can only move:
/// the module holds no button or key between calls, so a glide is all it
/// plays.
pub(crate) struct RealPointer<'a>(pub(crate) &'a Desktop);

impl InputSink for RealPointer<'_> {
    type Error = DesktopResponse;

    fn move_to(&mut self, point: Point) -> Result<(), DesktopResponse> {
        let reply = self.0.mouse_move(MouseMoveRequest {
            x: point.x,
            y: point.y,
        });
        if reply.ok { Ok(()) } else { Err(reply) }
    }

    fn press(&mut self, _button: Button) -> Result<(), DesktopResponse> {
        Err(unsupported("press"))
    }

    fn release(&mut self, _button: Button) -> Result<(), DesktopResponse> {
        Err(unsupported("release"))
    }

    fn key_down(&mut self, _key: Key) -> Result<(), DesktopResponse> {
        Err(unsupported("key-down"))
    }

    fn key_up(&mut self, _key: Key) -> Result<(), DesktopResponse> {
        Err(unsupported("key-up"))
    }

    fn text(&mut self, _text: &str) -> Result<(), DesktopResponse> {
        Err(unsupported("text"))
    }
}

fn unsupported(command: &str) -> DesktopResponse {
    DesktopResponse::err(
        command,
        tinydesktop_bus::DesktopError::new(
            "ACTION_NOT_SUPPORTED",
            "the real pointer only glides; the action itself goes through accessibility",
        ),
    )
}

/// A candidate's bounds, as a snapshot with `include_bounds` reports them.
pub(crate) fn bounds(target: &Candidate) -> Option<Rect> {
    let bounds = target.bounds.as_ref()?;
    let field = |name: &str| bounds.get(name).and_then(Value::as_f64);
    let rect = Rect::new(field("x")?, field("y")?, field("width")?, field("height")?);
    (rect.is_valid() && rect.width > 0.0 && rect.height > 0.0).then_some(rect)
}

/// The glide onto `target`, when this desktop would perform one: headed, a
/// moving profile, and a target with bounds.
pub(crate) fn glide_plan(desktop: &Desktop, target: &Candidate) -> Option<Plan> {
    if !desktop.is_headed() || desktop.motion().is_instant() {
        return None;
    }
    let rect = bounds(target)?;
    let mut mouse = desktop.pointer().0.lock().ok()?;
    Some(mouse.approach(rect).0)
}

/// Glides the real pointer onto `target`, when this desktop moves it.
pub(crate) fn glide_onto(desktop: &Desktop, target: &Candidate) {
    let Some(plan) = glide_plan(desktop, target) else {
        return;
    };
    let mut pacer = Pacer::new(std::thread::sleep);
    let mut wait = |pause| pacer.wait(pause);
    if play(&plan, &mut RealPointer(desktop), &mut wait).is_err()
        && let Ok(mut mouse) = desktop.pointer().0.lock()
    {
        // Where the pointer stopped is unknown; the next glide enters afresh.
        mouse.forget();
    }
}
