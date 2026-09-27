//! A virtual mouse and a virtual keyboard that move like a person.
//!
//! Automation that teleports the pointer onto an element's centre and
//! presses there skips everything a page or application sees on the way:
//! `mouseover`, `mouseenter`, `pointermove`, hover styles, menus that open on
//! hover. Text inserted in one go skips per-key handlers. This crate plans
//! input the way a hand produces it:
//!
//! - [`VirtualMouse`] aims somewhere inside an element (not dead centre),
//!   reaches it along a curved path that overshoots and corrects, with a
//!   gentle wobble and a small tremor, in the time Fitts's law gives, then
//!   settles before pressing and holds the button for a human beat.
//! - [`VirtualKeyboard`] presses one key per character with log-normal gaps,
//!   faster common letter pairs, and pauses after punctuation.
//! - [`MotionProfile`] sets the tempo; [`MotionProfile::Instant`] turns all
//!   of it off.
//!
//! Both produce a [`Plan`]: plain data, reproducible from a seed. [`play`]
//! performs a plan against any [`InputSink`] — CDP events for a browser page,
//! synthetic OS events for a desktop — so the humanization is written once
//! and every engine only supplies move, press, release, and key primitives.
//!
//! The crate deliberately holds no engine, no I/O, and no rendering. Drawing
//! the virtual cursor belongs to whoever owns the screen; the plan's moves
//! are exactly the positions to draw.
//!
//! ```
//! use tinydesktop_input::{Button, MotionProfile, Point, Rect, Rng, VirtualMouse, play};
//! # use tinydesktop_input::{InputSink, Key};
//! # #[derive(Default)] struct Log(Vec<String>);
//! # impl InputSink for Log {
//! #     type Error = ();
//! #     fn move_to(&mut self, p: Point) -> Result<(), ()> { self.0.push(format!("move {:.0},{:.0}", p.x, p.y)); Ok(()) }
//! #     fn press(&mut self, _: Button) -> Result<(), ()> { self.0.push("press".into()); Ok(()) }
//! #     fn release(&mut self, _: Button) -> Result<(), ()> { self.0.push("release".into()); Ok(()) }
//! #     fn key_down(&mut self, _: Key) -> Result<(), ()> { Ok(()) }
//! #     fn key_up(&mut self, _: Key) -> Result<(), ()> { Ok(()) }
//! #     fn text(&mut self, _: &str) -> Result<(), ()> { Ok(()) }
//! # }
//!
//! let mut mouse = VirtualMouse::with_rng(MotionProfile::Natural, Rng::seeded(1))
//!     .at(Point::new(40.0, 40.0));
//! let button = Rect::new(400.0, 300.0, 120.0, 32.0);
//! let plan = mouse.click(button, Button::Left, 1);
//!
//! assert!(plan.moves().len() > 10, "the pointer travels, it does not jump");
//! assert!(button.contains(mouse.position().unwrap()));
//!
//! let mut sink = Log::default();
//! play(&plan, &mut sink, &mut |_| {}).unwrap();
//! assert_eq!(sink.0[sink.0.len() - 2..], ["press", "release"]);
//! ```

mod error;
mod geometry;
mod keyboard;
mod mouse;
mod plan;
mod profile;
mod rng;

pub use error::{Error, Result};
pub use geometry::{Point, Rect};
pub use keyboard::VirtualKeyboard;
pub use mouse::{PathSample, VirtualMouse, aim, human_path};
pub use plan::{Button, InputSink, Key, Pacer, Plan, Step, play};
pub use profile::MotionProfile;
pub use rng::Rng;
