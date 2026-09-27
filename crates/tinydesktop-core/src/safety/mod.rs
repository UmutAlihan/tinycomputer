//! What an action would commit to, and whether a page is asking for payment.
//!
//! Two deterministic checks every surface runs before it acts, independently
//! of anything a model decided:
//!
//! - [`consequence`] classifies a control by its label. A payment is always
//!   stopped at; an irreversible action (send, delete, publish, confirm a
//!   booking) needs explicit approval; everything else proceeds. Stepping
//!   through a booking — "Book", "Select", "Continue" — is deliberately
//!   *reversible*: those lead to further forms, and the payment check stops
//!   the run before anything is charged.
//! - [`payment_evidence`] looks at the page itself — card fields, `cc-*`
//!   autocomplete attributes, a checkout URL with a pay button — so a payment
//!   step is caught even when its button says only "Continue".

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

/// Classifies a control by its visible label.
///
/// An empty label is [`Consequence::Irreversible`]: a control that says
/// nothing about itself cannot be shown to be harmless.
///
/// ```
/// use tinydesktop_core::{Consequence, consequence};
///
/// assert_eq!(consequence("Pay ₹6,840"), Consequence::Payment);
/// assert_eq!(consequence("Send"), Consequence::Irreversible);
/// assert_eq!(consequence("Book"), Consequence::Reversible);
/// assert_eq!(consequence("Continue to traveller details"), Consequence::Reversible);
/// ```
#[must_use]
pub fn consequence(label: &str) -> Consequence {
    let words = normalize(label);
    if words.trim().is_empty() {
        return Consequence::Irreversible;
    }
    if contains_any(&words, PAYMENT) {
        Consequence::Payment
    } else if contains_any(&words, IRREVERSIBLE) {
        Consequence::Irreversible
    } else {
        Consequence::Reversible
    }
}

/// One form field as a page describes it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldHint {
    /// The accessible label or placeholder.
    pub label: String,
    /// The HTML `autocomplete` attribute, when present.
    pub autocomplete: Option<String>,
    /// The HTML `name` or `id`, when present.
    pub name: Option<String>,
}

/// Why a page was judged to be asking for payment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentEvidence {
    /// Each signal found, in plain words, for the checkpoint a caller sees.
    pub reasons: Vec<String>,
}

/// Field wording that only a card form uses.
const CARD_FIELDS: &[&str] = &[
    "card number",
    "credit card",
    "debit card",
    "cardholder",
    "name on card",
    "cvv",
    "cvc",
    "cvv2",
    "security code",
    "card verification",
    "expiry date",
    "expiration date",
    "valid thru",
    "upi id",
    "vpa",
];

/// URL path words that mark a payment step.
const PAYMENT_PATHS: &[&str] = &["payment", "payments", "pay", "billing", "checkout"];

/// Whether the page is a payment step, with the reasons.
///
/// A card field (by label, name, or `cc-*` autocomplete) is enough on its
/// own. Without one, a payment URL *and* a payment control together are.
///
/// ```
/// use tinydesktop_core::{FieldHint, payment_evidence};
///
/// let card = FieldHint { autocomplete: Some("cc-number".into()), ..FieldHint::default() };
/// assert!(payment_evidence("https://ota.test/traveller", &[card], &[]).is_some());
/// assert!(payment_evidence("https://ota.test/results", &[], &["Book"]).is_none());
/// ```
#[must_use]
pub fn payment_evidence(
    url: &str,
    fields: &[FieldHint],
    controls: &[&str],
) -> Option<PaymentEvidence> {
    let mut reasons = Vec::new();
    for field in fields {
        if let Some(autocomplete) = field.autocomplete.as_deref().filter(|value| {
            value
                .to_ascii_lowercase()
                .split_whitespace()
                .any(|token| token.starts_with("cc-"))
        }) {
            reasons.push(format!("a card field (autocomplete {autocomplete})"));
            continue;
        }
        let described = normalize(&format!(
            "{} {}",
            field.label,
            field
                .name
                .as_deref()
                .unwrap_or_default()
                .replace(['_', '-'], " ")
        ));
        if let Some(term) = CARD_FIELDS.iter().find(|term| has_phrase(&described, term)) {
            reasons.push(format!("a card field ({term})"));
        }
    }
    let path = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .split_once('/')
        .map_or("", |(_, path)| path)
        .to_ascii_lowercase();
    let path_words = normalize(&path);
    let payment_url = PAYMENT_PATHS
        .iter()
        .any(|word| has_phrase(&path_words, word));
    let pay_control = controls
        .iter()
        .find(|label| consequence(label) == Consequence::Payment);
    if reasons.is_empty() {
        if let (true, Some(control)) = (payment_url, pay_control) {
            reasons.push(format!("a payment address with a {control:?} control"));
        }
    }
    (!reasons.is_empty()).then_some(PaymentEvidence { reasons })
}

/// Lower-case words separated by single spaces, padded so whole-word phrases
/// can be matched with `contains`.
fn normalize(text: &str) -> String {
    let words = text
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>();
    format!(
        " {} ",
        words.split_whitespace().collect::<Vec<_>>().join(" ")
    )
}

fn has_phrase(words: &str, phrase: &str) -> bool {
    words.contains(&format!(" {phrase} "))
}

fn contains_any(words: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|phrase| has_phrase(words, phrase))
}

#[cfg(test)]
mod test;
