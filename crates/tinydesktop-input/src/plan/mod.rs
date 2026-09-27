//! [`Plan`]: a gesture as a timed list of device primitives, and [`play`],
//! which performs one against an [`InputSink`].
//!
//! Planning and playing are separate so a gesture is deterministic data a
//! test can inspect, and so each engine only implements the primitives —
//! move, press, release, key down, key up, insert text — while the pacing
//! stays in one place.

use std::time::Duration;

use crate::geometry::Point;

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Button {
    /// The primary button.
    #[default]
    Left,
    /// The secondary button.
    Right,
    /// The wheel button.
    Middle,
}

impl Button {
    /// The button's name as CDP and agent-browser spell it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Middle => "middle",
        }
    }
}

/// A key the virtual keyboard presses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// A key that produces this character.
    Char(char),
    /// Return / Enter.
    Enter,
    /// Tab.
    Tab,
}

impl Key {
    /// The key's DOM `key` value: the character itself, or its name.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Char(character) => character.to_string(),
            Self::Enter => "Enter".to_owned(),
            Self::Tab => "Tab".to_owned(),
        }
    }

    /// The text the key inserts when pressed.
    #[must_use]
    pub fn text(self) -> String {
        match self {
            Self::Char(character) => character.to_string(),
            Self::Enter => "\r".to_owned(),
            Self::Tab => "\t".to_owned(),
        }
    }
}

/// One primitive of a gesture.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Move the pointer to a point.
    Move(Point),
    /// Press a button where the pointer is.
    Press(Button),
    /// Release a button where the pointer is.
    Release(Button),
    /// Press a key.
    KeyDown(Key),
    /// Release a key.
    KeyUp(Key),
    /// Insert text at once, with no key events.
    Text(String),
    /// Wait this many milliseconds.
    Pause(f64),
}

/// A gesture: primitives in order, with the pauses between them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plan {
    steps: Vec<Step>,
}

impl Plan {
    /// An empty plan.
    #[must_use]
    pub const fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// The primitives in order.
    #[must_use]
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Whether the plan does nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// How long the plan takes to play, counting only its pauses.
    #[must_use]
    pub fn duration_ms(&self) -> f64 {
        self.steps
            .iter()
            .map(|step| match step {
                Step::Pause(ms) => *ms,
                _ => 0.0,
            })
            .sum()
    }

    /// Every point the pointer visits, in order.
    #[must_use]
    pub fn moves(&self) -> Vec<Point> {
        self.steps
            .iter()
            .filter_map(|step| match step {
                Step::Move(point) => Some(*point),
                _ => None,
            })
            .collect()
    }

    /// Appends a step. A non-positive pause is dropped, and consecutive
    /// pauses merge, so a plan never sleeps twice in a row.
    pub fn push(&mut self, step: Step) {
        if let Step::Pause(ms) = step {
            if ms.is_nan() || ms <= 0.0 {
                return;
            }
            if let Some(Step::Pause(previous)) = self.steps.last_mut() {
                *previous += ms;
                return;
            }
        }
        self.steps.push(step);
    }

    /// Appends every step of `other`.
    pub fn extend(&mut self, other: Self) {
        for step in other.steps {
            self.push(step);
        }
    }
}

/// The primitives an engine must provide for a plan to play against it.
///
/// Each call performs one primitive immediately; [`play`] owns the timing.
pub trait InputSink {
    /// What a failed primitive reports.
    type Error;

    /// Moves the pointer to `point`.
    ///
    /// # Errors
    ///
    /// Whatever the engine reports when the move fails.
    fn move_to(&mut self, point: Point) -> Result<(), Self::Error>;

    /// Presses `button` where the pointer is.
    ///
    /// # Errors
    ///
    /// Whatever the engine reports when the press fails.
    fn press(&mut self, button: Button) -> Result<(), Self::Error>;

    /// Releases `button` where the pointer is.
    ///
    /// # Errors
    ///
    /// Whatever the engine reports when the release fails.
    fn release(&mut self, button: Button) -> Result<(), Self::Error>;

    /// Presses `key`.
    ///
    /// # Errors
    ///
    /// Whatever the engine reports when the key event fails.
    fn key_down(&mut self, key: Key) -> Result<(), Self::Error>;

    /// Releases `key`.
    ///
    /// # Errors
    ///
    /// Whatever the engine reports when the key event fails.
    fn key_up(&mut self, key: Key) -> Result<(), Self::Error>;

    /// Inserts `text` at once.
    ///
    /// # Errors
    ///
    /// Whatever the engine reports when the insertion fails.
    fn text(&mut self, text: &str) -> Result<(), Self::Error>;
}

/// Plays `plan` against `sink`, calling `wait` for every pause.
///
/// `wait` is a parameter so a test can play a plan without sleeping; a real
/// caller passes [`std::thread::sleep`].
///
/// # Errors
///
/// Stops at, and returns, the first primitive the sink fails.
pub fn play<S: InputSink + ?Sized>(
    plan: &Plan,
    sink: &mut S,
    wait: &mut dyn FnMut(Duration),
) -> Result<(), S::Error> {
    for step in plan.steps() {
        match step {
            Step::Move(point) => sink.move_to(*point)?,
            Step::Press(button) => sink.press(*button)?,
            Step::Release(button) => sink.release(*button)?,
            Step::KeyDown(key) => sink.key_down(*key)?,
            Step::KeyUp(key) => sink.key_up(*key)?,
            Step::Text(text) => sink.text(text)?,
            Step::Pause(ms) => wait(Duration::from_secs_f64(ms / 1_000.0)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod test;
