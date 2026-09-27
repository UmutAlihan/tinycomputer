//! [`ScreenCursor`]: one agent cursor for the whole screen, shared by every
//! surface.
//!
//! The desktop and a browser window are both just places on the screen, so
//! the agent gets one cursor that glides from a Mail button to a button in a
//! web page and back without jumping. Each surface converts its target to
//! global screen points and calls [`ScreenCursor::show`]; the cursor plans
//! the glide from wherever it last landed and hands it to an
//! [`OverlaySink`] to draw.
//!
//! The default sink is the `tinydesktop-cursor-overlay` helper process,
//! started on first use. A host with its own UI can supply a sink instead and
//! draw the cursor itself. Everything here is best effort: when nothing can
//! draw, actions are not slowed down and nothing fails.

mod process;

pub use process::{HELPER_ENV, HELPER_NAME, ProcessOverlay};

use std::sync::Mutex;
use std::time::Duration;

use crate::geometry::Rect;
use crate::glide::VirtualCursor;
use crate::pace::CursorPace;
use crate::protocol::OverlayCommand;

/// Something that draws the cursor.
pub trait OverlaySink: Send {
    /// Delivers one command.
    ///
    /// # Errors
    ///
    /// Any failure to deliver; the cursor then stops drawing.
    fn send(&mut self, command: &OverlayCommand) -> std::io::Result<()>;
}

/// How a [`ScreenCursor`] gets a sink the first time it needs one.
type Connect = Box<dyn Fn() -> Option<Box<dyn OverlaySink>> + Send + Sync>;

enum Link {
    /// Not tried yet.
    Pending,
    Open(Box<dyn OverlaySink>),
    /// Tried and failed: stays off rather than retrying on every action.
    Broken,
}

/// The agent's one cursor on screen.
pub struct ScreenCursor {
    pace: CursorPace,
    cursor: Mutex<VirtualCursor>,
    link: Mutex<Link>,
    connect: Connect,
    wait: fn(Duration),
}

impl std::fmt::Debug for ScreenCursor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScreenCursor")
            .field("pace", &self.pace)
            .finish_non_exhaustive()
    }
}

impl ScreenCursor {
    /// A cursor at `pace`, drawn by the overlay helper — found at `helper`,
    /// else through [`HELPER_ENV`], else as [`HELPER_NAME`] next to the
    /// running executable or on `PATH` — started the first time it is shown.
    #[must_use]
    pub fn new(pace: CursorPace, helper: Option<std::path::PathBuf>) -> Self {
        Self::with_connect(
            pace,
            Box::new(move || {
                ProcessOverlay::spawn(helper.as_deref())
                    .ok()
                    .map(|overlay| Box::new(overlay) as Box<dyn OverlaySink>)
            }),
        )
    }

    /// A cursor at `pace` drawn by `sink`, for a host that renders it itself.
    #[must_use]
    pub fn with_sink(pace: CursorPace, sink: Box<dyn OverlaySink>) -> Self {
        let cursor = Self::with_connect(pace, Box::new(|| None));
        if let Ok(mut link) = cursor.link.lock() {
            *link = Link::Open(sink);
        }
        cursor
    }

    /// A cursor that is never drawn.
    #[must_use]
    pub fn off() -> Self {
        Self::with_connect(CursorPace::Off, Box::new(|| None))
    }

    fn with_connect(pace: CursorPace, connect: Connect) -> Self {
        Self {
            pace,
            cursor: Mutex::new(VirtualCursor::new(pace)),
            link: Mutex::new(Link::Pending),
            connect,
            wait: std::thread::sleep,
        }
    }

    /// The same cursor, not waiting for glides to land, so tests run at full
    /// speed.
    #[must_use]
    pub fn without_waiting(mut self) -> Self {
        self.wait = |_| {};
        self
    }

    /// The pace glides are drawn at.
    #[must_use]
    pub const fn pace(&self) -> CursorPace {
        self.pace
    }

    fn send(&self, command: &OverlayCommand) -> bool {
        let Ok(mut link) = self.link.lock() else {
            return false;
        };
        if matches!(*link, Link::Pending) {
            *link = (self.connect)().map_or(Link::Broken, Link::Open);
        }
        let Link::Open(sink) = &mut *link else {
            return false;
        };
        if sink.send(command).is_ok() {
            return true;
        }
        *link = Link::Broken;
        false
    }

    /// Glides the cursor onto `target`, in global screen points, and waits
    /// for it to land so the action that follows is seen where it happens.
    /// Returns at once when the pace is off or nothing can draw.
    pub fn show(&self, target: Rect) {
        if self.pace.is_off() || !target.is_valid() || target.width <= 0.0 || target.height <= 0.0 {
            return;
        }
        let Some(glide) = self
            .cursor
            .lock()
            .ok()
            .and_then(|mut cursor| cursor.glide(target))
        else {
            return;
        };
        if self.send(&OverlayCommand::glide(&glide)) {
            (self.wait)(Duration::from_secs_f64(glide.duration_ms() / 1_000.0));
        } else if let Ok(mut cursor) = self.cursor.lock() {
            cursor.forget();
        }
    }

    /// Fades the cursor out, as when a run ends. The next glide fades it back
    /// in from nearby.
    pub fn hide(&self) {
        if self.pace.is_off() {
            return;
        }
        if let Ok(mut cursor) = self.cursor.lock() {
            if cursor.position().is_none() {
                return;
            }
            cursor.forget();
        }
        self.send(&OverlayCommand::Hide);
    }
}

#[cfg(test)]
mod test;
