//! Crate-wide error and result types.
//!
//! # What is, and is not, an error here
//!
//! A desktop command that fails is not a Rust error. A stale ref, a missing
//! permission, an application that is not running — those are *results*, and
//! they come back as a [`tinydesktop_bus::DesktopError`] inside the response
//! envelope, with a code, a suggestion, and a recovery hint a caller can act
//! on. Collapsing them into this enum would throw that structure away.
//!
//! What is left, and what this enum covers, is the module failing to reach the
//! point of running a command at all: a configuration blob it cannot read, or a
//! session directory it cannot open. Those are genuine construction failures,
//! and a caller cannot recover from them by trying a different ref.
//!
//! Variants carry the data a caller needs to react, keep their `#[error]`
//! message lowercase and free of trailing punctuation, and are documented so
//! the rendered rustdoc explains when each one occurs.

/// Errors returned by this crate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The module configuration was not a JSON object.
    ///
    /// The loader hands a module whatever configuration the host recorded for
    /// it. An array or a string is a host-side mistake worth naming rather
    /// than ignoring.
    #[error("module configuration must be a json object")]
    ConfigNotAnObject,

    /// A module configuration field held the wrong type.
    #[error("module configuration field `{field}` must be {expected}")]
    ConfigFieldType {
        /// The offending field's name.
        field: &'static str,
        /// What the field should have held, for example `a string`.
        expected: &'static str,
    },

    /// The engine could not establish a command context.
    ///
    /// Carries the engine's own message, which names the state directory or
    /// session it could not reach.
    #[error("cannot establish a desktop command context: {0}")]
    Context(String),
}

/// The crate's standard result type.
///
/// Use this alias in public signatures instead of spelling out
/// `std::result::Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod test;
