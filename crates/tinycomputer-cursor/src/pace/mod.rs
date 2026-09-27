//! [`CursorPace`]: whether the agent's cursor is drawn, and how fast it moves.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// How the agent's cursor moves on screen.
///
/// Every pace but [`Off`](CursorPace::Off) glides along the same human path —
/// curved, overshooting and correcting, with a small tremor. The paces differ
/// only in tempo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorPace {
    /// No cursor is drawn.
    Off,
    /// A practised user in a hurry.
    Brisk,
    /// An attentive user at an ordinary pace.
    #[default]
    Natural,
    /// A careful user, slow enough to follow easily.
    Calm,
}

impl CursorPace {
    /// Every pace, fastest first.
    pub const ALL: [Self; 4] = [Self::Off, Self::Brisk, Self::Natural, Self::Calm];

    /// The pace's configuration name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Brisk => "brisk",
            Self::Natural => "natural",
            Self::Calm => "calm",
        }
    }

    /// Whether the cursor is drawn at all.
    #[must_use]
    pub const fn is_off(self) -> bool {
        matches!(self, Self::Off)
    }

    /// The multiplier this pace applies to the natural travel time.
    #[must_use]
    pub(crate) const fn tempo(self) -> f64 {
        match self {
            Self::Off => 0.0,
            Self::Brisk => 0.6,
            Self::Natural => 1.0,
            Self::Calm => 1.6,
        }
    }
}

impl fmt::Display for CursorPace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for CursorPace {
    type Err = Error;

    /// Parses a configuration name, ignoring case and surrounding space.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownPace`] for any other name.
    fn from_str(name: &str) -> Result<Self> {
        let wanted = name.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|pace| pace.as_str() == wanted)
            .ok_or_else(|| Error::UnknownPace {
                name: name.to_owned(),
            })
    }
}

#[cfg(test)]
mod test;
