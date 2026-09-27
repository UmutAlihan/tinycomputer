//! [`Animator`]: what the overlay draws at each moment.
//!
//! The overlay's platform code is only a window that can move, show one of
//! the sprite's frames, and fade. Everything about timing — following a
//! glide's path, fading in, pulsing on landing, hiding once the agent has
//! gone quiet — is decided here, where it is testable without a display.

use crate::geometry::Point;
use crate::protocol::OverlayCommand;

/// How long the cursor takes to fade in or out, in milliseconds.
pub const FADE_MS: f64 = 150.0;

/// How long the landing pulse lasts, in milliseconds.
pub const PULSE_MS: f64 = 450.0;

/// How long the cursor lingers after its last glide before it fades out:
/// long enough to see where it acted, short enough not to be left behind.
pub const IDLE_MS: f64 = 6_000.0;

/// One moment of the overlay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// Where the cursor's tip is, in screen points.
    pub position: Point,
    /// How opaque the cursor is, from 0 to 1.
    pub opacity: f64,
    /// How far the landing pulse has run, from 0 to 1, while it runs.
    pub pulse: Option<f64>,
}

/// The overlay's state over time. Times are milliseconds on any clock that
/// only moves forward.
#[derive(Debug, Clone, Default)]
pub struct Animator {
    path: Vec<[f64; 3]>,
    started: f64,
    appears: bool,
    hidden_at: Option<f64>,
}

impl Animator {
    /// An animator that shows nothing yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes in a command at time `now`.
    pub fn apply(&mut self, command: OverlayCommand, now: f64) {
        match command {
            OverlayCommand::Glide { path, appears } if !path.is_empty() => {
                // A glide that interrupts a hidden or fading cursor fades in.
                let visible = self.frame(now).is_some_and(|frame| frame.opacity > 0.0);
                self.appears = appears || !visible;
                self.path = path;
                self.started = now;
                self.hidden_at = None;
            }
            OverlayCommand::Glide { .. } => {}
            OverlayCommand::Hide => {
                if self.hidden_at.is_none() && !self.path.is_empty() {
                    self.hidden_at = Some(now);
                }
            }
        }
    }

    fn duration(&self) -> f64 {
        self.path.last().map_or(0.0, |last| last[0])
    }

    fn position(&self, elapsed: f64) -> Option<Point> {
        let first = self.path.first()?;
        let last = self.path.last()?;
        if elapsed <= first[0] {
            return Some(Point::new(first[1], first[2]));
        }
        if elapsed >= last[0] {
            return Some(Point::new(last[1], last[2]));
        }
        let next = self.path.iter().position(|sample| sample[0] > elapsed)?;
        let (a, b) = (self.path[next - 1], self.path[next]);
        let k = if b[0] > a[0] {
            (elapsed - a[0]) / (b[0] - a[0])
        } else {
            1.0
        };
        Some(Point::new(
            a[1] + (b[1] - a[1]) * k,
            a[2] + (b[2] - a[2]) * k,
        ))
    }

    /// What to draw at `now`; `None` when nothing is on screen.
    #[must_use]
    pub fn frame(&self, now: f64) -> Option<Frame> {
        let elapsed = now - self.started;
        let position = self.position(elapsed)?;
        let landed = elapsed - self.duration();
        let hide_from = self
            .hidden_at
            .unwrap_or(self.started + self.duration() + IDLE_MS);
        let fading = now - hide_from;
        if fading >= FADE_MS {
            return None;
        }
        let fade_in = if self.appears {
            (elapsed / FADE_MS).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let fade_out = (1.0 - fading / FADE_MS).clamp(0.0, 1.0);
        let pulse = (landed >= 0.0 && landed < PULSE_MS).then(|| landed / PULSE_MS);
        Some(Frame {
            position,
            opacity: fade_in.min(fade_out),
            pulse,
        })
    }

    /// Whether the picture is still changing at `now`, so the overlay needs
    /// frequent ticks; when it is not, the overlay can wait for commands.
    #[must_use]
    pub fn is_moving(&self, now: f64) -> bool {
        let Some(frame) = self.frame(now) else {
            return false;
        };
        let elapsed = now - self.started;
        frame.pulse.is_some() || frame.opacity < 1.0 || elapsed < self.duration()
    }

    /// When the picture next changes on its own after `now`: the idle
    /// fade-out, or `None` when nothing is on screen.
    #[must_use]
    pub fn next_change(&self, now: f64) -> Option<f64> {
        self.frame(now)?;
        let hide_from = self
            .hidden_at
            .unwrap_or(self.started + self.duration() + IDLE_MS);
        Some(hide_from.max(now))
    }
}

#[cfg(test)]
mod test;
