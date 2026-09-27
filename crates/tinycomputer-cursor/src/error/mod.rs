//! Crate-wide error and result types.

/// Errors returned by this crate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A pace name that is not one of `off`, `brisk`, `natural`, or `calm`.
    #[error("unknown cursor pace `{name}`, expected off, brisk, natural, or calm")]
    UnknownPace {
        /// The name as it was given.
        name: String,
    },
}

/// The crate's standard result type.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod test;
