//! Request payloads for the members that synthesize keyboard and mouse input.
//!
//! These are the escape hatch, not the default. A member here drives the real
//! cursor or the real keyboard, so it necessarily takes focus and disturbs
//! whoever is using the machine — which is exactly why the
//! [`crate::interaction`] members exist and should be preferred wherever an
//! element has an accessibility action.
//!
//! Reach for these when the accessibility tree cannot express what has to
//! happen: a global shortcut ([`PressRequest`]), a canvas that only understands
//! pixels ([`MouseClickRequest`]), a drag between two elements
//! ([`DragRequest`]).
//!
//! # The four members that always fail
//!
//! `KeyDown`, `KeyUp`, `MouseDown`, and `MouseUp` are served, validate their
//! arguments, and then fail closed with a suggestion naming the member to use
//! instead. Holding a button down is a stateful operation and this module is
//! stateless: nothing keeps the button held once a call returns, so a
//! `MouseDown` that appeared to succeed would be a lie the caller only
//! discovers through a broken drag. They are part of the surface so the
//! failure is a structured, explained reply rather than an `UnknownMethod`,
//! and so they are already named when a stateful daemon can implement them.

mod types;

pub use types::{
    DragEndpoint, DragRequest, HoldKeyRequest, HoldMouseRequest, HoverRequest, MouseClickRequest,
    MouseMoveRequest, MouseWheelRequest, PressRequest,
};

#[cfg(test)]
mod test;
