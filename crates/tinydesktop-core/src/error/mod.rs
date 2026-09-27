//! Crate-wide error and result types.

/// Errors returned by this crate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A fact looked like payment-card data, which a task never holds.
    ///
    /// Carries the fact's name only; the value is never repeated.
    #[error("fact `{name}` looks like payment card data, which tasks never hold")]
    CardDataRefused {
        /// The offending fact's name.
        name: String,
    },
}

/// The crate's standard result type.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod test;
