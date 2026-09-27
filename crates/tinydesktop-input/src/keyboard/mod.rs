//! [`VirtualKeyboard`]: text typed key by key at a human cadence.
//!
//! Each character becomes a key press, a hold, a release, and a gap before
//! the next, so a page sees `keydown`, `keypress`, `input`, and `keyup` for
//! every character the way it does from a person, and autocomplete, input
//! masks, and per-key validation behave as they would for one.

mod cadence;

use crate::plan::{Key, Plan, Step};
use crate::profile::MotionProfile;
use crate::rng::Rng;

/// A virtual keyboard that plans typing.
#[derive(Debug, Clone)]
pub struct VirtualKeyboard {
    profile: MotionProfile,
    rng: Rng,
}

impl VirtualKeyboard {
    /// A keyboard typing at `profile`, seeded from the system.
    #[must_use]
    pub fn new(profile: MotionProfile) -> Self {
        Self::with_rng(profile, Rng::from_entropy())
    }

    /// A keyboard typing at `profile` whose timing `rng` decides.
    #[must_use]
    pub const fn with_rng(profile: MotionProfile, rng: Rng) -> Self {
        Self { profile, rng }
    }

    /// The profile typing is planned with.
    #[must_use]
    pub const fn profile(&self) -> MotionProfile {
        self.profile
    }

    /// Types `text`.
    ///
    /// The instant profile inserts the whole text in one step; every other
    /// profile presses a key per character, turning `\n` into Enter and `\t`
    /// into Tab. Carriage returns are dropped, so `\r\n` is one Enter.
    pub fn type_text(&mut self, text: &str) -> Plan {
        let mut plan = Plan::new();
        if text.is_empty() {
            return plan;
        }
        if self.profile.is_instant() {
            plan.push(Step::Text(text.to_owned()));
            return plan;
        }
        let mut previous: Option<char> = None;
        for character in text.chars().filter(|character| *character != '\r') {
            if let Some(previous) = previous {
                plan.push(Step::Pause(cadence::gap_ms(
                    previous,
                    character,
                    self.profile,
                    &mut self.rng,
                )));
            }
            let key = match character {
                '\n' => Key::Enter,
                '\t' => Key::Tab,
                other => Key::Char(other),
            };
            plan.push(Step::KeyDown(key));
            plan.push(Step::Pause(cadence::hold_ms(self.profile, &mut self.rng)));
            plan.push(Step::KeyUp(key));
            previous = Some(character);
        }
        plan
    }
}

#[cfg(test)]
mod test;
