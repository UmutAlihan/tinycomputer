//! How long a typist holds each key and waits before the next.
//!
//! Inter-key intervals are log-normal around a tempo's median: mostly
//! steady, occasionally slow. Common letter pairs roll off the fingers
//! faster, a capital costs a shift, punctuation and the start of a word cost
//! a moment's thought, and now and then the typist pauses to think.

use crate::profile::MotionProfile;
use crate::rng::Rng;

/// The median gap between keys at the natural tempo, in milliseconds: a
/// little under 60 words a minute.
pub(crate) const MEDIAN_GAP_MS: f64 = 95.0;

/// Gaps never fall below or rise above these multiples of the median,
/// before a thinking pause is added.
pub(crate) const GAP_BOUNDS: (f64, f64) = (0.35, 4.0);

/// The longest thinking pause, in milliseconds at the natural tempo.
pub(crate) const THINK_MS: (f64, f64) = (200.0, 500.0);

/// How long a key is held, in milliseconds at the natural tempo.
pub(crate) const HOLD_MS: (f64, f64) = (45.0, 140.0);

/// The pairs English typists roll fastest.
const FAST_BIGRAMS: [&str; 16] = [
    "th", "he", "in", "er", "an", "re", "on", "at", "en", "nd", "ti", "es", "or", "te", "ed", "is",
];

/// How long to hold a key.
pub(crate) fn hold_ms(profile: MotionProfile, rng: &mut Rng) -> f64 {
    rng.normal(85.0, 15.0).clamp(HOLD_MS.0, HOLD_MS.1) * profile.tempo()
}

/// How long to wait after `previous` before pressing `next`.
pub(crate) fn gap_ms(previous: char, next: char, profile: MotionProfile, rng: &mut Rng) -> f64 {
    let mut factor = rng.normal(0.0, 0.35).exp();
    let pair: String = [previous, next]
        .iter()
        .flat_map(|character| character.to_lowercase())
        .collect();
    if FAST_BIGRAMS.contains(&pair.as_str()) {
        factor *= 0.75;
    }
    if next.is_uppercase() {
        factor *= 1.2;
    }
    if previous.is_whitespace() {
        factor *= 1.15;
    }
    if matches!(previous, '.' | ',' | ';' | ':' | '!' | '?') {
        factor *= 1.8;
    }
    let mut gap = MEDIAN_GAP_MS * factor.clamp(GAP_BOUNDS.0, GAP_BOUNDS.1);
    if previous.is_whitespace() && !next.is_whitespace() && rng.chance(0.03) {
        gap += rng.range(THINK_MS.0, THINK_MS.1);
    }
    gap * profile.tempo()
}
