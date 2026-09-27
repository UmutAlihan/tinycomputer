//! [`ScreenCursor`]: one agent cursor for the whole screen, shared by every
//! surface.
//!
//! The desktop and a browser window are both just places on the screen, so
//! the agent gets one cursor that glides from a Mail button to a button in a
//! web page and back without jumping. Each surface converts its target to
//! global screen points and calls [`ScreenCursor::arrive`]; the cursor plans
//! the glide from wherever it last landed, hands it to an [`OverlaySink`] to
//! draw, and returns when the cursor lands — so the action the surface then
//! performs happens as the cursor arrives, in time with its landing pulse,
//! like a click. The cursor is cosmetic: the action itself is unchanged.
//!
//! The default sink is the `tinycomputer-cursor-overlay` helper process,
//! started on first use and written to from a background thread. Everything
//! here is best effort, and only a glide that was actually delivered is
//! waited for: with no helper, a failed one, or one too far behind to take
//! the glide, the action goes ahead at once. A host with its own UI can
//! supply a sink instead and draw the cursor itself.

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
    /// [`std::io::ErrorKind::WouldBlock`] when the command was dropped
    /// because the sink is busy — the cursor keeps the sink but does not wait
    /// for that glide; any other error when the sink is gone, and the cursor
    /// then stops drawing.
    fn send(&mut self, command: &OverlayCommand) -> std::io::Result<()>;
}

/// How long a freshly started helper takes to draw its sprite and show its
/// window, added to the first glide's wait so the first action still lands
/// with the cursor.
const HELPER_STARTUP: Duration = Duration::from_millis(300);

/// Whether a command reached the sink.
enum Sent {
    /// Delivered; `fresh` when the sink was started by this very send.
    Delivered { fresh: bool },
    /// Not delivered; nothing will be drawn for it.
    Lost,
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

    /// The same cursor, returning from [`ScreenCursor::arrive`] without
    /// waiting, so tests run at full speed.
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

    fn send(&self, command: &OverlayCommand) -> Sent {
        let Ok(mut link) = self.link.lock() else {
            return Sent::Lost;
        };
        let fresh = matches!(*link, Link::Pending);
        if fresh {
            *link = (self.connect)().map_or(Link::Broken, Link::Open);
        }
        let Link::Open(sink) = &mut *link else {
            return Sent::Lost;
        };
        match sink.send(command) {
            Ok(()) => Sent::Delivered { fresh },
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Sent::Lost,
            Err(_) => {
                *link = Link::Broken;
                Sent::Lost
            }
        }
    }

    /// Starts the cursor gliding onto `target`, in global screen points, and
    /// returns at once with how long it will take to land — `None` when
    /// nothing will be drawn. [`ScreenCursor::arrive`] is the same, waiting
    /// for the landing.
    pub fn show(&self, target: Rect) -> Option<Duration> {
        if self.pace.is_off() || !target.is_valid() || target.width <= 0.0 || target.height <= 0.0 {
            return None;
        }
        let glide = self
            .cursor
            .lock()
            .ok()
            .and_then(|mut cursor| cursor.glide(target))?;
        match self.send(&OverlayCommand::glide(&glide)) {
            Sent::Delivered { fresh } => {
                let travel = Duration::from_secs_f64(glide.duration_ms() / 1_000.0);
                Some(if fresh {
                    travel + HELPER_STARTUP
                } else {
                    travel
                })
            }
            Sent::Lost => {
                if let Ok(mut cursor) = self.cursor.lock() {
                    cursor.forget();
                }
                None
            }
        }
    }

    /// Glides the cursor onto `target`, in global screen points, and returns
    /// once it lands, so the action that follows happens as the cursor
    /// arrives — in time with its landing pulse, like a click.
    ///
    /// Returns at once when nothing will be drawn: the pace is off, the
    /// target has no area, or no overlay took the glide.
    pub fn arrive(&self, target: Rect) {
        if let Some(landing) = self.show(target) {
            (self.wait)(landing);
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
        let _sent = self.send(&OverlayCommand::Hide);
    }
}

#[cfg(test)]
mod test;
