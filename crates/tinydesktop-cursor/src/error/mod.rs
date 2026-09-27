//! Crate-wide error and result types.

/// Errors returned by this crate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A motion profile name that is not one of `instant`, `brisk`,
    /// `natural`, or `calm`.
    #[error("unknown motion profile `{name}`, expected instant, brisk, natural, or calm")]
    UnknownProfile {
        /// The name as it was given.
        name: String,
    },
}

/// The crate's standard result type.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod test;
