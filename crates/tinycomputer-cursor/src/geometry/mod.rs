//! Points and rectangles in whatever coordinate space the caller works in:
//! global screen points for a desktop, CSS pixels of the viewport for a page.

/// A position.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    /// Horizontal position, growing rightwards.
    pub x: f64,
    /// Vertical position, growing downwards.
    pub y: f64,
}

impl Point {
    /// The point at `(x, y)`.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// The straight-line distance to `other`.
    #[must_use]
    pub fn distance(self, other: Self) -> f64 {
        (other.x - self.x).hypot(other.y - self.y)
    }

    /// Whether both coordinates are finite numbers.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    pub(crate) fn plus(self, dx: f64, dy: f64) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }

    pub(crate) fn lerp(self, other: Self, t: f64) -> Self {
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }
}

/// An axis-aligned rectangle: an element's bounds.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width; a rectangle with none is a point.
    pub width: f64,
    /// Height; a rectangle with none is a point.
    pub height: f64,
}

impl Rect {
    /// The rectangle whose top-left corner is `(x, y)`.
    #[must_use]
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The rectangle a single point occupies.
    #[must_use]
    pub const fn at(point: Point) -> Self {
        Self::new(point.x, point.y, 0.0, 0.0)
    }

    /// The middle of the rectangle.
    #[must_use]
    pub fn center(self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Whether `point` lies inside the rectangle, edges included.
    #[must_use]
    pub fn contains(self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.width
            && point.y >= self.y
            && point.y <= self.y + self.height
    }

    /// Whether every field is finite and neither side is negative.
    #[must_use]
    pub fn is_valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
            && self.width >= 0.0
            && self.height >= 0.0
    }
}

#[cfg(test)]
mod geometry_tests;
