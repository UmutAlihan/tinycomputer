//! Classifying a control by its label: payment, irreversible, or reversible.

use crate::surface::{Candidate, Screen};
use super::{contains_any, has_phrase, normalize};

/// What pressing a control commits the user to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Consequence {
    /// Nothing that cannot be undone or navigated away from.
    Reversible,
    /// Something that cannot be taken back: a message sent, data deleted, a
    /// post published, a reservation confirmed.
    Irreversible,
    /// Money changes hands.
    Payment,
}

/// Words that mean money is about to move, as whole-word phrases.
const PAYMENT: &[&str] = &[
    "pay",
    "pay now",
    "payment",
    "make payment",
    "complete payment",
    "proceed to payment",
    "purchase",
    "buy",
    "buy now",
    "place order",
    "checkout",
    "check out",
    "complete purchase",
    "confirm and pay",
    "authorize",
    "subscribe",
    "donate",
    "transfer",
];

/// Words that mean something cannot be taken back.
const IRREVERSIBLE: &[&str] = &[
    "send",
    "delete",
    "remove",
    "discard",
    "erase",
    "empty trash",
    "publish",
    "post",
    "share",
    "invite",
    "submit",
    "sign",
    "sign out",
    "log out",
    "unsubscribe",
    "overwrite",
    "replace",
    "quit without saving",
    "confirm booking",
    "confirm reservation",
    "complete booking",
    "complete reservation",
    "cancel booking",
    "cancel reservation",
    "cancel subscription",
    "close account",
    "deactivate",
];

/// Verbs that lower a count.
const DECREASE: &[&str] = &["remove", "decrease", "reduce", "minus", "subtract"];

/// What a booking counts: a stepper lowering one of these changes a number.
const COUNTED: &[&str] = &[
    "adult",
    "adults",
    "child",
    "children",
    "infant",
    "infants",
    "passenger",
    "passengers",
    "traveller",
    "travellers",
    "traveler",
    "travelers",
    "guest",
    "guests",
    "room",
    "rooms",
];

/// Whether `label` is a counter's minus button — "Remove Adult, 2 Adult
/// Remaining", "Decrease adults" — which only changes a number that its plus
/// button changes back, however it is worded.
///
/// ```
/// use tinycomputer_core::adjusts_a_count;
///
/// assert!(adjusts_a_count("Remove Adult, 2 Adult Remaining"));
/// assert!(!adjusts_a_count("Remove passenger details"));
/// assert!(!adjusts_a_count("Remove"));
/// ```
#[must_use]
pub fn adjusts_a_count(label: &str) -> bool {
    let words = normalize(label);
    let words = words.split_whitespace().collect::<Vec<_>>();
    words.windows(2).any(|pair| {
        DECREASE.contains(&pair[0])
            && COUNTED.contains(&pair[1])
            && !words
                .iter()
                .any(|word| matches!(*word, "details" | "information" | "info"))
    })
}

/// Classifies a control by its visible label.
///
/// An empty label is [`Consequence::Irreversible`]: a control that says
/// nothing about itself cannot be shown to be harmless.
///
/// ```
/// use tinycomputer_core::{Consequence, consequence};
///
/// assert_eq!(consequence("Pay ₹6,840"), Consequence::Payment);
/// assert_eq!(consequence("Send"), Consequence::Irreversible);
/// assert_eq!(consequence("Book"), Consequence::Reversible);
/// assert_eq!(consequence("Continue to traveller details"), Consequence::Reversible);
/// assert_eq!(consequence("Remove Adult"), Consequence::Reversible);
/// ```
#[must_use]
pub fn consequence(label: &str) -> Consequence {
    let words = normalize(label);
    if words.trim().is_empty() {
        return Consequence::Irreversible;
    }
    if contains_any(&words, PAYMENT) {
        Consequence::Payment
    } else if contains_any(&words, IRREVERSIBLE) && !adjusts_a_count(label) {
        Consequence::Irreversible
    } else {
        Consequence::Reversible
    }
}
