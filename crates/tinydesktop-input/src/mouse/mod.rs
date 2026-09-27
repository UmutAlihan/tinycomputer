//! [`VirtualMouse`]: a pointer that reaches, dwells, and presses like a hand.
//!
//! The mouse remembers where it is, so each gesture starts where the last one
//! ended, and turns an element's bounds into a [`Plan`] of moves, pauses, and
//! button events. It performs nothing itself; [`play`](crate::play) does, on
//! whatever engine the caller drives.

mod aim;
mod path;

pub use aim::aim;
pub use path::{PathSample, human_path};

use std::f64::consts::TAU;

use crate::geometry::{Point, Rect};
use crate::plan::{Button, Plan, Step};
use crate::profile::MotionProfile;
use crate::rng::Rng;

/// Where a mouse with no known position appears, as a distance from its
/// first target: far enough that the first reach is visible.
const ENTRY_DISTANCE: (f64, f64) = (240.0, 420.0);

/// How long a hand settles on a target before pressing, in milliseconds.
const SETTLE_MS: (f64, f64) = (60.0, 140.0);

/// How long a hover lingers, long enough for hover styles and menus.
const HOVER_MS: (f64, f64) = (250.0, 450.0);

/// How long a button stays down in a click.
const HOLD_MS: (f64, f64) = (55.0, 110.0);

/// The gap between the clicks of a double click.
const MULTI_CLICK_GAP_MS: (f64, f64) = (90.0, 150.0);

/// How long a drag holds still after grabbing and before dropping.
const GRAB_MS: (f64, f64) = (120.0, 200.0);

/// A virtual pointer that plans human gestures.
#[derive(Debug, Clone)]
pub struct VirtualMouse {
    profile: MotionProfile,
    rng: Rng,
    position: Option<Point>,
}

impl VirtualMouse {
    /// A mouse moving at `profile`, seeded from the system.
    #[must_use]
    pub fn new(profile: MotionProfile) -> Self {
        Self::with_rng(profile, Rng::from_entropy())
    }

    /// A mouse moving at `profile` whose gestures `rng` decides; a seeded
    /// generator makes them reproducible.
    #[must_use]
    pub const fn with_rng(profile: MotionProfile, rng: Rng) -> Self {
        Self {
            profile,
            rng,
            position: None,
        }
    }

    /// The same mouse, starting at `position`.
    #[must_use]
    pub const fn at(mut self, position: Point) -> Self {
        self.position = Some(position);
        self
    }

    /// The profile gestures are planned with.
    #[must_use]
    pub const fn profile(&self) -> MotionProfile {
        self.profile
    }

    /// Where the pointer is, once it has moved or been placed.
    #[must_use]
    pub const fn position(&self) -> Option<Point> {
        self.position
    }

    /// Records that the pointer is at `position`, as when the engine reports
    /// it or something else moved it.
    pub fn place(&mut self, position: Point) {
        self.position = Some(position);
    }

    /// Forgets the position, as when the page or window underneath changed.
    pub fn forget(&mut self) {
        self.position = None;
    }

    /// A point on `target` where a hand would aim; its centre for the
    /// instant profile, which is what automation did before.
    pub fn aim(&mut self, target: Rect) -> Point {
        if self.profile.is_instant() {
            return target.center();
        }
        aim(target, &mut self.rng)
    }

    /// A reach to `to`, a target `width` wide.
    ///
    /// A mouse with no position first appears a short distance away, so the
    /// reach is visible and hover events fire on the way in.
    pub fn glide(&mut self, to: Point, width: f64) -> Plan {
        let mut plan = Plan::new();
        let start = match self.position {
            Some(start) => start,
            None if self.profile.is_instant() => to,
            None => {
                let entry = self.entry(to);
                plan.push(Step::Move(entry));
                entry
            }
        };
        let mut elapsed = 0.0;
        for sample in human_path(start, to, width, self.profile, &mut self.rng) {
            plan.push(Step::Pause(sample.t_ms - elapsed));
            plan.push(Step::Move(sample.point));
            elapsed = sample.t_ms;
        }
        self.position = Some(to);
        plan
    }

    /// Reaches onto `target` and settles, ready to press — what a click is
    /// before its button goes down. Returns the plan and the point aimed at.
    pub fn approach(&mut self, target: Rect) -> (Plan, Point) {
        let point = self.aim(target);
        let mut plan = self.glide(point, target.width.min(target.height));
        let settle = self.pause(SETTLE_MS);
        plan.push(settle);
        (plan, point)
    }

    /// Reaches onto `target` and lingers there.
    pub fn hover(&mut self, target: Rect) -> Plan {
        let point = self.aim(target);
        let mut plan = self.glide(point, target.width.min(target.height));
        let linger = self.pause(HOVER_MS);
        plan.push(linger);
        plan
    }

    /// Reaches onto `target` and clicks `button` `count` times.
    pub fn click(&mut self, target: Rect, button: Button, count: u8) -> Plan {
        let (mut plan, _) = self.approach(target);
        for click in 0..count.max(1) {
            if click > 0 {
                let gap = self.pause(MULTI_CLICK_GAP_MS);
                plan.push(gap);
            }
            plan.push(Step::Press(button));
            let hold = self.pause(HOLD_MS);
            plan.push(hold);
            plan.push(Step::Release(button));
        }
        plan
    }

    /// Grabs `from`, carries it to `to`, and drops it.
    pub fn drag(&mut self, from: Rect, to: Rect) -> Plan {
        let (mut plan, _) = self.approach(from);
        plan.push(Step::Press(Button::Left));
        let grab = self.pause(GRAB_MS);
        plan.push(grab);
        let drop = self.aim(to);
        plan.extend(self.glide(drop, to.width.min(to.height)));
        let hold = self.pause(GRAB_MS);
        plan.push(hold);
        plan.push(Step::Release(Button::Left));
        plan
    }

    fn pause(&mut self, (low, high): (f64, f64)) -> Step {
        Step::Pause(self.rng.range(low, high) * self.profile.tempo())
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
