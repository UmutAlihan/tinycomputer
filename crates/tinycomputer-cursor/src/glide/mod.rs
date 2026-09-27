//! [`VirtualCursor`]: the agent's cursor, gliding from where it last was onto
//! the element an action is about to land on.
//!
//! The cursor is only ever drawn. It sends no input: the engine performs the
//! action exactly as it would with no cursor on screen, and the cursor shows
//! where that action lands. A [`Glide`] is the plan a renderer animates —
//! timed positions, reproducible from a seed.

mod aim;
mod path;

pub use aim::aim;
pub use path::{PathSample, human_path};

use std::f64::consts::TAU;

use crate::geometry::{Point, Rect};
use crate::pace::CursorPace;
use crate::rng::Rng;

/// Where a cursor with no known position appears, as a distance from its
/// first target: far enough that the first glide is visible.
const ENTRY_DISTANCE: (f64, f64) = (240.0, 420.0);

/// One animation of the cursor onto a target.
#[derive(Debug, Clone, PartialEq)]
pub struct Glide {
    /// Where the glide starts. When [`Glide::appears`] is set the cursor was
    /// not on screen before and fades in here.
    pub from: Point,
    /// Where on the target the cursor lands.
    pub to: Point,
    /// Timed positions from `from` to `to`; the last is exactly `to`.
    pub samples: Vec<PathSample>,
    /// Whether the cursor appears at `from` rather than already being there.
    pub appears: bool,
}

impl Glide {
    /// How long the glide takes, in milliseconds.
    #[must_use]
    pub fn duration_ms(&self) -> f64 {
        self.samples.last().map_or(0.0, |sample| sample.t_ms)
    }
}

/// The agent's cursor: remembers where it is and plans each glide from there.
#[derive(Debug, Clone)]
pub struct VirtualCursor {
    pace: CursorPace,
    rng: Rng,
    position: Option<Point>,
}

impl VirtualCursor {
    /// A cursor moving at `pace`, seeded from the system.
    #[must_use]
    pub fn new(pace: CursorPace) -> Self {
        Self::with_rng(pace, Rng::from_entropy())
    }

    /// A cursor moving at `pace` whose paths `rng` decides; a seeded
    /// generator makes them reproducible.
    #[must_use]
    pub const fn with_rng(pace: CursorPace, rng: Rng) -> Self {
        Self {
            pace,
            rng,
            position: None,
        }
    }

    /// The same cursor, starting at `position`.
    #[must_use]
    pub const fn at(mut self, position: Point) -> Self {
        self.position = Some(position);
        self
    }

    /// The pace glides are planned at.
    #[must_use]
    pub const fn pace(&self) -> CursorPace {
        self.pace
    }

    /// Where the cursor is, once it has glided or been placed.
    #[must_use]
    pub const fn position(&self) -> Option<Point> {
        self.position
    }

    /// Records that the cursor is at `position`.
    pub fn place(&mut self, position: Point) {
        self.position = Some(position);
    }

    /// Forgets the position, as when the page or window underneath changed
    /// and the cursor is no longer on screen.
    pub fn forget(&mut self) {
        self.position = None;
    }

    /// The glide onto `target`: aimed near, not at, its centre, along a
    /// human path from wherever the cursor is. `None` when the pace is off.
    ///
    /// A cursor with no position appears a short distance from the target,
    /// so its first glide is visible too.
    pub fn glide(&mut self, target: Rect) -> Option<Glide> {
        if self.pace.is_off() {
            return None;
        }
        let to = aim(target, &mut self.rng);
        let (from, appears) = match self.position {
            Some(from) => (from, false),
            None => (self.entry(to), true),
        };
        let width = target.width.min(target.height);
        let samples = human_path(from, to, width, self.pace, &mut self.rng);
        self.position = Some(to);
        Some(Glide {
            from,
            to,
            samples,
            appears,
        })
    }

    fn entry(&mut self, to: Point) -> Point {
        let angle = self.rng.range(0.0, TAU);
        let reach = self.rng.range(ENTRY_DISTANCE.0, ENTRY_DISTANCE.1);
        Point::new(
            (to.x + reach * angle.cos()).max(0.0),
            (to.y + reach * angle.sin()).max(0.0),
        )
    }
}

#[cfg(test)]
mod test;
