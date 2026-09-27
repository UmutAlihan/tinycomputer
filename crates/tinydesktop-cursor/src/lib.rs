//! The agent's on-screen cursor.
//!
//! When an agent acts on a page or an application, a watching person sees
//! nothing move: the engine clicks an element directly. This crate plans a
//! second cursor, drawn over the content, that shows the agent at work:
//!
//! - [`VirtualCursor`] aims somewhere inside the element about to be acted on
//!   (near, not dead on, its centre) and glides there from where it last was.
//! - The path is a hand's: one quick stroke that bows sideways and lands a
//!   little past the target, a short correction back onto it, a gentle wobble
//!   and a small tremor, in the time Fitts's law gives for the distance and
//!   the target's size ([`human_path`]).
//! - [`CursorPace`] sets the tempo; [`CursorPace::Off`] draws nothing.
//!
//! The cursor is purely cosmetic. It sends no input and never touches the
//! user's own pointer: the engine performs every action exactly as it would
//! with no cursor on screen. A [`Glide`] is plain data — timed positions,
//! reproducible from a seed — for whatever owns the screen to animate.
//!
//! ```
//! use tinydesktop_cursor::{CursorPace, Point, Rect, Rng, VirtualCursor};
//!
//! let mut cursor = VirtualCursor::with_rng(CursorPace::Natural, Rng::seeded(1))
//!     .at(Point::new(40.0, 40.0));
//! let button = Rect::new(400.0, 300.0, 120.0, 32.0);
//! let glide = cursor.glide(button).expect("a natural cursor glides");
//!
//! assert!(glide.samples.len() > 10, "the cursor travels, it does not jump");
//! assert!(button.contains(glide.to));
//! assert_eq!(glide.samples.last().map(|sample| sample.point), Some(glide.to));
//! ```

mod error;
mod geometry;
mod glide;
mod pace;
mod rng;

pub use error::{Error, Result};
pub use geometry::{Point, Rect};
pub use glide::{Glide, PathSample, VirtualCursor, aim, human_path};
pub use pace::CursorPace;
pub use rng::Rng;
