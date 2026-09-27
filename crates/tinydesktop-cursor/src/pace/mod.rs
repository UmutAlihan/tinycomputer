//! [`MotionProfile`]: how quickly and how humanly the virtual devices move.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// How the virtual mouse and keyboard pace themselves.
///
/// Every profile but [`Instant`](MotionProfile::Instant) moves along a curved
/// path with overshoot and tremor, pauses the way a hand does before it
/// presses, and types key by key. The profiles differ only in tempo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionProfile {
    /// No motion: the pointer jumps and text is inserted at once. The
    /// behaviour before virtual input existed, for tests and bulk work.
    Instant,
    /// A practised user in a hurry.
    Brisk,
    /// An attentive user at an ordinary pace.
    #[default]
    Natural,
    /// A careful user, slow enough to follow on screen.
    Calm,
}

impl MotionProfile {
    /// Every profile, fastest first.
    pub const ALL: [Self; 4] = [Self::Instant, Self::Brisk, Self::Natural, Self::Calm];

    /// The profile's configuration name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Instant => "instant",
            Self::Brisk => "brisk",
            Self::Natural => "natural",
            Self::Calm => "calm",
        }
    }

    /// Whether this profile moves at all.
    #[must_use]
    pub const fn is_instant(self) -> bool {
        matches!(self, Self::Instant)
    }

    /// The multiplier this profile applies to every human timing: pointer
    /// travel, dwell, key holds, and the gaps between keys.
    #[must_use]
    pub(crate) const fn tempo(self) -> f64 {
        match self {
            Self::Instant => 0.0,
            Self::Brisk => 0.6,
            Self::Natural => 1.0,
            Self::Calm => 1.6,
        }
    }
}

impl fmt::Display for MotionProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for MotionProfile {
    type Err = Error;

    /// Parses a configuration name, ignoring case and surrounding space.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownProfile`] for any other name.
    fn from_str(name: &str) -> Result<Self> {
        let wanted = name.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|profile| profile.as_str() == wanted)
            .ok_or_else(|| Error::UnknownProfile {
                name: name.to_owned(),
            })
    }
}

#[cfg(test)]
mod test;
