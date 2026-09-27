//! Where on an element a hand aims.
//!
//! People click near, not at, the middle of a control, and almost never on
//! its edge. The aim point is drawn around the centre and kept to the inner
//! part of the element, so it always lands on the element itself.

use crate::geometry::{Point, Rect};
use crate::rng::Rng;

/// Share of each side an aim point stays clear of: aim lands in the
/// middle 60% of the width and middle 50% of the height.
const MARGIN: (f64, f64) = (0.2, 0.25);

/// Spread of aim points around the centre, as a share of each side.
const SPREAD: (f64, f64) = (0.12, 0.15);

/// Elements smaller than this on either side are aimed at dead centre.
const MIN_SIDE: f64 = 2.0;

/// A point inside `target` where a hand would aim.
///
/// Always inside the rectangle; the centre for a point-sized or invalid one.
#[must_use]
pub fn aim(target: Rect, rng: &mut Rng) -> Point {
    let center = target.center();
    if !target.is_valid() || target.width < MIN_SIDE || target.height < MIN_SIDE {
        return center;
    }
    let x = rng.normal(center.x, target.width * SPREAD.0).clamp(
        target.x + target.width * MARGIN.0,
        target.x + target.width * (1.0 - MARGIN.0),
    );
    let y = rng.normal(center.y, target.height * SPREAD.1).clamp(
        target.y + target.height * MARGIN.1,
        target.y + target.height * (1.0 - MARGIN.1),
    );
    Point::new(x, y)
}
