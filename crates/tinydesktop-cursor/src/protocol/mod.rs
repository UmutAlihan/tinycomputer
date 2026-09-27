//! What the module tells the overlay: one JSON object per line.
//!
//! The overlay is a separate process (`tinydesktop-cursor-overlay`), so the
//! cursor keeps drawing whichever surface — desktop or browser — the agent is
//! on, and a crash in drawing can never take the module down. Coordinates are
//! global screen points, origin at the top-left of the primary display.

use serde::{Deserialize, Serialize};

use crate::glide::Glide;

/// One instruction to the overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OverlayCommand {
    /// Animate the cursor along `path`, `[t_ms, x, y]` triples whose first is
    /// the starting point, then pulse where it lands.
    Glide {
        /// Timed positions; the first has `t_ms` 0.
        path: Vec<[f64; 3]>,
        /// Whether the cursor fades in at the start rather than already
        /// being there.
        appears: bool,
    },
    /// Fade the cursor out.
    Hide,
}

impl OverlayCommand {
    /// The command that animates `glide`, with positions rounded to a tenth
    /// of a point.
    #[must_use]
    pub fn glide(glide: &Glide) -> Self {
        let round = |value: f64| (value * 10.0).round() / 10.0;
        let path = std::iter::once([0.0, round(glide.from.x), round(glide.from.y)])
            .chain(glide.samples.iter().map(|sample| {
                [
                    round(sample.t_ms),
                    round(sample.point.x),
                    round(sample.point.y),
                ]
            }))
            .collect();
        Self::Glide {
            path,
            appears: glide.appears,
        }
    }

    /// The command as one line of the protocol, newline included.
    #[must_use]
    pub fn to_line(&self) -> String {
        let mut line = serde_json::to_string(self).unwrap_or_else(|_| r#"{"type":"hide"}"#.into());
        line.push('\n');
        line
    }

    /// Reads one line of the protocol; `None` for anything that is not a
    /// command, which the overlay ignores rather than dying on.
    #[must_use]
    pub fn from_line(line: &str) -> Option<Self> {
        serde_json::from_str(line.trim()).ok()
    }
}

#[cfg(test)]
mod test;
