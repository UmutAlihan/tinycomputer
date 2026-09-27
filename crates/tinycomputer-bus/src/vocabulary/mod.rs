//! The small enumerations that appear inside more than one request payload.
//!
//! Every type here mirrors a `agent-desktop-core` enumeration by wire form
//! rather than by import. That is the point of the contract crate: a host links
//! this crate to name a surface or a modifier key without linking the engine,
//! the accessibility backend, or anything platform-specific. The module crate
//! owns the conversion in the other direction, and its unit tests pin every
//! variant so a rename upstream fails a build here instead of producing an
//! `INVALID_ARGS` reply in a host at runtime.

mod types;

pub use types::{
    ClipboardFormat, Direction, ElementProperty, ElementStateProperty, Modifier, MouseButton,
    StatePredicate, Surface,
};

#[cfg(test)]
mod test;
