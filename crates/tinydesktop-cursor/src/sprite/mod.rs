//! [`Sprite`]: the cursor's look, drawn once in Rust as RGBA frames.
//!
//! The overlay only puts pixels on screen; what the cursor looks like is
//! decided here, identically on every platform. Frame 0 is the cursor at
//! rest; the frames after it add the landing pulse, a ring that grows and
//! fades around the tip.
//!
//! The canvas is square with the cursor's tip — its hotspot — at the centre,
//! so the ring can spread in every direction and a window placed at
//! `tip - hotspot` puts the tip exactly on the target.

mod png;

pub use png::encode as png;

use crate::geometry::Point;

/// The canvas side, in points.
pub const SIZE: u32 = 64;

/// How many pulse frames follow the resting frame.
pub const PULSE_FRAMES: usize = 12;

/// [`PULSE_FRAMES`] as the float arithmetic's integer type.
const PULSE_STEPS: u32 = 12;

/// Subsamples per pixel side, for anti-aliasing.
const SUPERSAMPLE: u32 = 4;

/// The arrow's outline, in points from the tip.
const ARROW: [(f64, f64); 7] = [
    (0.0, 0.0),
    (0.0, 20.0),
    (5.4, 15.4),
    (9.2, 23.5),
    (12.6, 22.0),
    (8.8, 14.2),
    (16.2, 14.2),
];

/// The cursor's colour, a violet no system cursor uses.
const FILL: [f64; 3] = [124.0, 92.0, 255.0];

/// The outline that keeps the cursor legible on any background.
const OUTLINE: [f64; 3] = [255.0, 255.0, 255.0];

/// Outline width, in points.
const OUTLINE_WIDTH: f64 = 1.6;

/// The drop shadow's offset and softness, in points.
const SHADOW: (f64, f64, f64) = (0.0, 1.5, 3.0);

/// The cursor's appearance: frames of straight (non-premultiplied) RGBA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    /// Pixels per point the frames were drawn at.
    pub scale: u32,
    /// Frame 0 at rest, then [`PULSE_FRAMES`] of the landing pulse.
    pub frames: Vec<Vec<u8>>,
}

impl Sprite {
    /// The cursor drawn at `scale` pixels per point: 1 on an ordinary
    /// display, 2 on a Retina one.
    #[must_use]
    pub fn render(scale: u32) -> Self {
        let scale = scale.clamp(1, 4);
        let frames = (0..=PULSE_STEPS)
            .map(|frame| {
                let pulse = (frame > 0).then(|| f64::from(frame) / f64::from(PULSE_STEPS + 1));
                draw(scale, pulse)
            })
            .collect();
        Self { scale, frames }
    }

    /// The side of each frame, in pixels.
    #[must_use]
    pub const fn pixels(&self) -> u32 {
        SIZE * self.scale
    }

    /// Where the tip is on the canvas, in points.
    #[must_use]
    pub fn hotspot() -> Point {
        let middle = f64::from(SIZE) / 2.0;
        Point::new(middle, middle)
    }

    /// The frame to show for a pulse `progress` from 0 to 1, or at rest.
    #[must_use]
    pub fn frame_for(pulse: Option<f64>) -> usize {
        pulse.map_or(0, |progress| {
            let reached = progress.clamp(0.0, 1.0) * f64::from(PULSE_STEPS);
            (1..PULSE_FRAMES)
                .zip(1..PULSE_STEPS)
                .find(|&(_, step)| reached < f64::from(step))
                .map_or(PULSE_FRAMES, |(frame, _)| frame)
        })
    }

    /// Frame `index` as premultiplied BGRA, the layout Windows' layered
    /// windows take.
    #[must_use]
    pub fn premultiplied_bgra(&self, index: usize) -> Vec<u8> {
        self.frames
            .get(index)
            .map(|frame| {
                frame
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|&[red, green, blue, alpha]| {
                        let scale = |channel: u8| {
                            u8::try_from(u16::from(channel) * u16::from(alpha) / 255)
                                .unwrap_or(u8::MAX)
                        };
                        [scale(blue), scale(green), scale(red), alpha]
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Whether `(x, y)` is inside the polygon (even-odd rule).
fn inside(polygon: &[(f64, f64)], x: f64, y: f64) -> bool {
    let mut inside = false;
    let mut previous = polygon[polygon.len() - 1];
    for &point in polygon {
        if (point.1 > y) != (previous.1 > y)
            && x < (previous.0 - point.0) * (y - point.1) / (previous.1 - point.1) + point.0
        {
            inside = !inside;
        }
        previous = point;
    }
    inside
}

/// The distance from `(x, y)` to the polygon's outline.
fn distance(polygon: &[(f64, f64)], x: f64, y: f64) -> f64 {
    let mut nearest = f64::MAX;
    let mut previous = polygon[polygon.len() - 1];
    for &point in polygon {
        let (dx, dy) = (point.0 - previous.0, point.1 - previous.1);
        let length = dx * dx + dy * dy;
        let t = if length > 0.0 {
            (((x - previous.0) * dx + (y - previous.1) * dy) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (px, py) = (previous.0 + t * dx, previous.1 + t * dy);
        nearest = nearest.min((x - px).hypot(y - py));
        previous = point;
    }
    nearest
}

/// Paints straight-alpha `colour` at `alpha` over `under` (premultiplied
/// accumulator).
fn over(under: [f64; 4], colour: [f64; 3], alpha: f64) -> [f64; 4] {
    let keep = 1.0 - alpha;
    [
        colour[0] * alpha + under[0] * keep,
        colour[1] * alpha + under[1] * keep,
        colour[2] * alpha + under[2] * keep,
        alpha + under[3] * keep,
    ]
}

/// The colour at one subsample, premultiplied, in points from the tip.
fn sample(x: f64, y: f64, pulse: Option<f64>) -> [f64; 4] {
    let mut colour = [0.0; 4];
    if let Some(progress) = pulse {
        let radius = 4.0 + 20.0 * progress;
        let fade = 1.0 - progress;
        let from_tip = x.hypot(y);
        if from_tip <= radius {
            colour = over(colour, FILL, 0.2 * fade);
        }
        if (from_tip - radius).abs() <= 1.0 {
            colour = over(colour, FILL, 0.9 * fade);
        }
    }
    let shadow = distance(&ARROW, x - SHADOW.0, y - SHADOW.1);
    let shadowed = inside(&ARROW, x - SHADOW.0, y - SHADOW.1);
    if shadowed || shadow < SHADOW.2 {
        let depth = if shadowed {
            1.0
        } else {
            1.0 - shadow / SHADOW.2
        };
        colour = over(colour, [0.0, 0.0, 0.0], 0.35 * depth);
    }
    if inside(&ARROW, x, y) {
        let edge = distance(&ARROW, x, y);
        colour = over(
            colour,
            if edge < OUTLINE_WIDTH { OUTLINE } else { FILL },
            1.0,
        );
    } else if distance(&ARROW, x, y) < OUTLINE_WIDTH / 2.0 {
        colour = over(colour, OUTLINE, 1.0);
    }
    colour
}

/// One frame, straight RGBA, at `scale` pixels per point.
fn draw(scale: u32, pulse: Option<f64>) -> Vec<u8> {
    let side = SIZE * scale;
    let hotspot = Sprite::hotspot();
    let per_point = f64::from(scale);
    let steps = f64::from(SUPERSAMPLE);
    let mut pixels = Vec::with_capacity(usize::try_from(side * side * 4).unwrap_or(0));
    for row in 0..side {
        for column in 0..side {
            let mut sum = [0.0; 4];
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let x =
                        (f64::from(column) + (f64::from(sx) + 0.5) / steps) / per_point - hotspot.x;
                    let y =
                        (f64::from(row) + (f64::from(sy) + 0.5) / steps) / per_point - hotspot.y;
                    let colour = sample(x, y, pulse);
                    for (total, channel) in sum.iter_mut().zip(colour) {
                        *total += channel;
                    }
                }
            }
            let count = steps * steps;
            let alpha = sum[3] / count;
            let straight = |channel: f64| {
                if alpha > 0.0 {
                    channel / count / alpha
                } else {
                    0.0
                }
            };
            pixels.extend([
                byte(straight(sum[0])),
                byte(straight(sum[1])),
                byte(straight(sum[2])),
                byte(alpha * 255.0),
            ]);
        }
    }
    pixels
}

/// The nearest byte to `value`, clamped to 0–255.
fn byte(value: f64) -> u8 {
    let target = value.round().clamp(0.0, 255.0);
    let (mut low, mut high) = (0_u8, u8::MAX);
    while low < high {
        let middle = low + (high - low) / 2;
        if f64::from(middle) < target {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low
}

#[cfg(test)]
mod test;
