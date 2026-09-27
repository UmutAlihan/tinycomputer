//! The facts a task may type: the caller's values, held locally by name.
//!
//! A traveller's name, date of birth, email, and phone are what a booking
//! form needs, and none of them should reach a decision model or a planner.
//! Jev and the planner see [`Facts::names`]; a surface looks a value up with
//! [`Facts::get`] at the moment it types it; and [`Facts::redact`] scrubs any
//! value out of text before that text leaves the machine.
//!
//! Payment-card data is refused at the door, by name and by value, so no
//! task can ever be handed a card to type.

use std::collections::BTreeMap;

use crate::error::{Error, Result};

/// Names that only ever label card data.
const CARD_NAMES: &[&str] = &[
    "card number",
    "credit card",
    "debit card",
    "cvv",
    "cvc",
    "security code",
    "card expiry",
    "card expiration",
    "card pin",
    "upi pin",
];

/// A task's named values.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Facts {
    values: BTreeMap<String, String>,
}

impl std::fmt::Debug for Facts {
    /// Names only: a fact's value never reaches a log.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_set().entries(self.values.keys()).finish()
    }
}

impl Facts {
    /// Facts from `(name, value)` pairs.
    ///
    /// # Errors
    ///
    /// [`Error::CardDataRefused`] when a name labels card data, or a value is
    /// a plausible card number (13 to 19 digits passing the Luhn check).
    ///
    /// ```
    /// use tinydesktop_core::Facts;
    ///
    /// let facts = Facts::new([("email", "sam@example.com")]).unwrap();
    /// assert_eq!(facts.get("email"), Some("sam@example.com"));
    /// assert!(Facts::new([("card number", "4111 1111 1111 1111")]).is_err());
    /// ```
    pub fn new<N: Into<String>, V: Into<String>>(
        pairs: impl IntoIterator<Item = (N, V)>,
    ) -> Result<Self> {
        let mut values = BTreeMap::new();
        for (name, value) in pairs {
            let (name, value) = (name.into(), value.into());
            if labels_card_data(&name) || is_card_number(&value) {
                return Err(Error::CardDataRefused { name });
            }
            values.insert(name, value);
        }
        Ok(Self { values })
    }

    /// The value of the fact called `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// Every fact's name: all a model is ever shown.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.values.keys().map(String::as_str).collect()
    }

    /// The names in `wanted` that no fact supplies, in order.
    #[must_use]
    pub fn missing<'a>(&self, wanted: &'a [String]) -> Vec<&'a str> {
        wanted
            .iter()
            .map(String::as_str)
            .filter(|name| !self.values.contains_key(*name))
            .collect()
    }

    /// `text` with every fact value replaced by `‹name›`, longest values
    /// first so one value inside another is still replaced whole.
    #[must_use]
    pub fn redact(&self, text: &str) -> String {
        let mut facts = self
            .values
            .iter()
            .filter(|(_, value)| !value.trim().is_empty())
            .collect::<Vec<_>>();
        facts.sort_by_key(|(_, value)| std::cmp::Reverse(value.len()));
        facts
            .into_iter()
            .fold(text.to_owned(), |text, (name, value)| {
                text.replace(value.as_str(), &format!("‹{name}›"))
            })
    }
}

fn labels_card_data(name: &str) -> bool {
    let words = format!(
        " {} ",
        name.to_lowercase()
            .replace(|character: char| !character.is_alphanumeric(), " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    );
    CARD_NAMES
        .iter()
        .any(|term| words.contains(&format!(" {term} ")))
}

/// Whether `value` is 13 to 19 digits (spaces and dashes aside) that pass
/// the Luhn checksum every payment card number carries.
fn is_card_number(value: &str) -> bool {
    if value
        .chars()
        .any(|character| !(character.is_ascii_digit() || matches!(character, ' ' | '-')))
    {
        return false;
    }
    let digits = value
        .chars()
        .filter_map(|character| character.to_digit(10))
        .collect::<Vec<_>>();
    if !(13..=19).contains(&digits.len()) {
        return false;
    }
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(position, &digit)| {
            if position % 2 == 1 {
                let doubled = digit * 2;
                if doubled > 9 { doubled - 9 } else { doubled }
            } else {
                digit
            }
        })
        .sum();
    sum % 10 == 0
}

#[cfg(test)]
mod test;
