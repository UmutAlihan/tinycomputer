//! The path a hand takes from one point to another.
//!
//! A reach is modelled as one fast primary submovement that lands a little
//! past the target, then a short correction back onto it — the shape
//! minimum-jerk models of aimed movements predict and pointer traces show.
//! On top of that the primary stroke bows sideways, wobbles gently, and
//! carries a small tremor, all fading to nothing at both ends so the path
//! starts and finishes exactly where it should. How long it takes follows
//! Fitts's law: farther and smaller targets take longer.

use std::f64::consts::{PI, TAU};

use crate::geometry::Point;
use crate::profile::MotionProfile;
use crate::rng::Rng;

/// How often the pointer reports a position, as a 60 Hz display does.
pub(crate) const SAMPLE_MS: f64 = 16.0;

/// Reaches shorter than this carry no overshoot: a hand does not overshoot a
/// nudge.
const OVERSHOOT_MIN_DISTANCE: f64 = 80.0;

/// The farthest past its target a reach may land, as a share of distance.
pub(crate) const OVERSHOOT_MAX_SHARE: f64 = 0.04;

/// The farthest past its target a reach may land, in pixels.
const OVERSHOOT_MAX_PX: f64 = 24.0;

/// Share of the travel time the primary submovement takes when there is a
/// correction after it.
const PRIMARY_SHARE: f64 = 0.8;

/// A target is never treated as narrower than this in Fitts's law, so a
/// zero-width point still gets a sensible travel time.
const MIN_TARGET_WIDTH: f64 = 8.0;

/// The shortest and longest a reach takes at the natural tempo.
pub(crate) const TRAVEL_MS: (f64, f64) = (160.0, 1_100.0);

/// One reported pointer position, `t_ms` after the reach began.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathSample {
    /// Milliseconds since the reach began.
    pub t_ms: f64,
    /// Where the pointer is.
    pub point: Point,
}

/// How long a reach of `distance` towards a target `width` wide takes.
pub(crate) fn travel_ms(distance: f64, width: f64, profile: MotionProfile, rng: &mut Rng) -> f64 {
    let difficulty = (distance / width.max(MIN_TARGET_WIDTH) + 1.0).log2();
    let jitter = rng.normal(1.0, 0.08).clamp(0.85, 1.2);
    ((110.0 + 120.0 * difficulty) * jitter).clamp(TRAVEL_MS.0, TRAVEL_MS.1) * profile.tempo()
}

/// The minimum-jerk position profile: how far along a stroke a hand is at
/// normalized time `tau`, starting and stopping with zero speed.
fn minimum_jerk(tau: f64) -> f64 {
    let tau = tau.clamp(0.0, 1.0);
    tau.powi(3) * (10.0 - 15.0 * tau + 6.0 * tau * tau)
}

fn bezier(start: Point, control: Point, end: Point, s: f64) -> Point {
    start.lerp(control, s).lerp(control.lerp(end, s), s)
}

/// A reach from `from` to `to`, a target `width` wide, sampled at 60 Hz.
///
/// The last sample is exactly `to`, and sample times strictly increase. The
/// instant profile, and a reach too short to see, is a single sample at `to`.
#[must_use]
pub fn human_path(
    from: Point,
    to: Point,
    width: f64,
    profile: MotionProfile,
    rng: &mut Rng,
) -> Vec<PathSample> {
    let distance = from.distance(to);
    if profile.is_instant() || distance < 1.0 || !from.is_finite() || !to.is_finite() {
        return vec![PathSample {
            t_ms: 0.0,
            point: to,
        }];
    }
    let total = travel_ms(distance, width, profile, rng);
    let (dx, dy) = ((to.x - from.x) / distance, (to.y - from.y) / distance);
    let (nx, ny) = (-dy, dx);

    let overshoots = distance >= OVERSHOOT_MIN_DISTANCE && rng.chance(0.7);
    let landing = if overshoots {
        let past = (rng.normal(0.025, 0.01) * distance)
            .clamp(0.005 * distance, OVERSHOOT_MAX_SHARE * distance)
            .min(OVERSHOOT_MAX_PX);
        let aside = rng.normal(0.0, 0.01 * distance).clamp(-6.0, 6.0);
        to.plus(dx * past + nx * aside, dy * past + ny * aside)
    } else {
        to
    };
    let primary = if overshoots {
        total * PRIMARY_SHARE
    } else {
        total
    };

    let bow_limit = (0.25 * distance).min(120.0);
    let bow = rng.normal(0.0, 0.1 * distance).clamp(-bow_limit, bow_limit);
    let control = from.lerp(landing, 0.5).plus(nx * bow, ny * bow);

    let wobble = (0.012 * distance).min(5.0) * rng.range(0.5, 1.0);
    let wobble_cycles = rng.range(1.2, 2.2);
    let wobble_phase = rng.range(0.0, TAU);

    let tremor = 0.6 * rng.range(0.7, 1.3);
    // Physiological tremor sits around 8–12 Hz, independently per axis.
    let across = (rng.range(8.0, 12.0), rng.range(0.0, TAU));
    let down = (rng.range(8.0, 12.0), rng.range(0.0, TAU));

    let at = |t: f64| -> Point {
        let base = if t <= primary {
            let tau = t / primary;
            let lateral =
                wobble * (TAU * wobble_cycles * tau + wobble_phase).sin() * (PI * tau).sin();
            bezier(from, control, landing, minimum_jerk(tau)).plus(nx * lateral, ny * lateral)
        } else {
            landing.lerp(to, minimum_jerk((t - primary) / (total - primary)))
        };
        let fade = tremor * (PI * t / total).sin();
        let seconds = t / 1_000.0;
        base.plus(
            fade * (TAU * across.0 * seconds + across.1).sin(),
            fade * (TAU * down.0 * seconds + down.1).sin(),
        )
    };

    let mut samples = Vec::new();
    let mut t = SAMPLE_MS;
    while t < total - 1.0 {
        samples.push(PathSample {
            t_ms: t,
            point: at(t),
        });
        t += SAMPLE_MS;
    }
    samples.push(PathSample {
        t_ms: total,
        point: to,
    });
    samples
}
