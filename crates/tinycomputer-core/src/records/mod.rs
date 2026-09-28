//! Records extracted from a page, the values in them, and ranking.
//!
//! A results page — flights, hotels, products — is a list of repeated cards.
//! Once each card is a [`Record`] of named text fields, picking "the cheapest"
//! or "the earliest" is arithmetic, not judgement: [`rank`] does it
//! deterministically whenever the criterion names something these parsers can
//! read, and a decision model is asked only when it cannot.

use std::collections::BTreeMap;

mod price;
mod rank;
mod schedule;

pub use price::{Price, parse_price};
pub use rank::{Criterion, rank};
pub use schedule::{parse_clock, parse_duration, parse_stops};

/// One extracted item: field name to the text shown for it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    /// Field name, such as `price` or `departure`, to its visible text.
    pub fields: BTreeMap<String, String>,
}

impl Record {
    /// A record from `(name, text)` pairs.
    #[must_use]
    pub fn from_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        Self {
            fields: pairs
                .into_iter()
                .map(|(name, text)| (name.to_owned(), text.to_owned()))
                .collect(),
        }
    }

    /// The text of the first field whose name contains any of `hints`.
    fn field(&self, hints: &[&str]) -> Option<&str> {
        self.fields
            .iter()
            .find(|(name, _)| {
                let name = name.to_ascii_lowercase();
                hints.iter().any(|hint| name.contains(hint))
            })
            .map(|(_, text)| text.as_str())
    }
}

#[cfg(test)]
mod records_tests;
