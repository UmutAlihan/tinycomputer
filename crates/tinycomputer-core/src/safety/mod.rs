//! What an action would commit to, and whether a page is asking for payment.
//!
//! Two deterministic checks every surface runs before it acts, independently
//! of anything a model decided:
//!
//! - [`consequence`] classifies a control by its label. A payment is never
//!   made on its own — it is stopped at, or held for approval; an irreversible action (send, delete, publish, confirm a
//!   booking) needs explicit approval; everything else proceeds. Stepping
//!   through a booking — "Book", "Select", "Continue" — is deliberately
//!   *reversible*: those lead to further forms, and the payment check stops
//!   the run before anything is charged.
//! - [`payment_evidence`] looks at the page itself — card fields, `cc-*`
//!   autocomplete attributes, a checkout URL with a pay button — so a payment
//!   step is caught even when its button says only "Continue".
//! - [`screen_payment_evidence`] builds `payment_evidence`'s inputs from a
//!   surface-agnostic [`Screen`](crate::surface::Screen), so every surface's
//!   destructive-click gate can apply the same page-level check.

use crate::surface::{Candidate, Screen};

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
/// use tinycomputer_core::{FieldHint, payment_evidence};
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
    if reasons.is_empty()
        && payment_url
        && let Some(control) = pay_control
    {
        reasons.push(format!("a payment address with a {control:?} control"));
    }
    (!reasons.is_empty()).then_some(PaymentEvidence { reasons })
}

/// Card wording strong enough to mark a payment form from text beside a
/// field; promotions say "credit card" or "UPI", never "CVV".
const STRONG_CARD_WORDS: &[&str] = &[
    "card number",
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
    // A payment identifier, never a promotional phrase: an ad says "pay by
    // UPI", never "UPI ID" or "VPA" on their own.
    "upi id",
    "vpa",
];

/// Whether the screen a flow is looking at is a payment step, from its
/// candidates and text alone.
///
/// A generic [`Screen`](crate::surface::Screen) carries no URL and no HTML
/// `autocomplete` attribute — those are web-specific, and this check must
/// hold for the desktop too — so it reads fields: an input labelled like a
/// card field, or an input with strong card wording (CVV, card number)
/// beside it, within a few nodes of it in document order. A candidate with
/// no role and no actions counts as a possible input, so its label alone
/// can mark the page. Card words on links, buttons, or promotional text
/// alone ("save 10% with your credit card"), or strong card wording far
/// from every field, do not make a payment page.
///
/// ```
/// use tinycomputer_core::surface::{Candidate, Screen};
/// use tinycomputer_core::screen_payment_evidence;
///
/// let field = |name: &str| Candidate {
///     role: "textbox".to_owned(),
///     name: Some(name.to_owned()),
///     available_actions: vec!["SetValue".to_owned()],
///     ..Candidate::default()
/// };
/// let mut screen = Screen {
///     app: "browser".to_owned(),
///     window: None,
///     surface: "window".to_owned(),
///     candidates: vec![field("Card number")],
///     context: Vec::new(),
///     unexplored: Vec::new(),
///     text_nodes: Vec::new(),
/// };
/// assert!(screen_payment_evidence(&screen).is_some());
/// screen.candidates = vec![field("Traveller name")];
/// screen.context = vec!["Pay less with your credit card".to_owned()];
/// assert!(screen_payment_evidence(&screen).is_none());
/// ```
#[must_use]
pub fn screen_payment_evidence(screen: &Screen) -> Option<PaymentEvidence> {
    let fields = screen
        .candidates
        .iter()
        .filter(|candidate| may_take_input(candidate))
        .collect::<Vec<_>>();
    if fields.is_empty() {
        return None;
    }
    let labelled = fields.iter().filter_map(|field| label(field));
    // `screen.context` is left out: it repeats `text_nodes`' text without
    // their place in the tree, so it cannot show the wording is beside a
    // field rather than in a footer or help panel.
    let beside = screen
        .text_nodes
        .iter()
        .chain(
            screen
                .candidates
                .iter()
                .filter(|candidate| !may_take_input(candidate)),
        )
        .filter(|node| {
            fields
                .iter()
                .any(|field| node.order.abs_diff(field.order) <= NEARBY_NODES)
        })
        .filter_map(label)
        .filter(|text| {
            let words = normalize(text);
            STRONG_CARD_WORDS
                .iter()
                .any(|term| has_phrase(&words, term))
        });
    let hints = labelled
        .chain(beside)
        .map(|text| FieldHint {
            label: text.to_owned(),
            ..FieldHint::default()
        })
        .collect::<Vec<_>>();
    payment_evidence("", &hints, &[])
}

/// How many nodes apart, in document order, wording and a field may sit and
/// still be read as the field's label: room for the wrappers between a
/// label and its input, not for a footer far down the page.
const NEARBY_NODES: usize = 5;

/// A candidate's name, or its description when it has no name.
fn label(candidate: &Candidate) -> Option<&str> {
    candidate
        .name
        .as_deref()
        .or(candidate.description.as_deref())
}

/// Whether a candidate may take typed input, on either surface: it says it
/// does, or it carries no role and no actions, so nothing says it does not.
fn may_take_input(candidate: &Candidate) -> bool {
    let unmarked = candidate.role.is_empty() && candidate.available_actions.is_empty();
    unmarked
        || candidate
            .available_actions
            .iter()
            .any(|action| action == "SetValue" || action == "TypeText")
        || matches!(
            candidate.role.as_str(),
            "textbox" | "searchbox" | "combobox" | "spinbutton" | "textfield"
        )
}

/// What only a person can get past, by the words a page shows for it.
const HUMAN_GATES: &[(&str, &str)] = &[
    ("captcha", "solve the captcha"),
    ("recaptcha", "solve the captcha"),
    ("verify you are human", "prove you are human"),
    ("verify you re human", "prove you are human"),
    ("i m not a robot", "prove you are human"),
    ("are you a robot", "prove you are human"),
    ("one time password", "enter the one-time password"),
    ("enter the otp", "enter the one-time password"),
    ("verification code", "enter the verification code"),
    ("enter the code we sent", "enter the verification code"),
    ("two factor", "complete two-factor authentication"),
    ("2 step verification", "complete two-factor authentication"),
    ("sign in to continue", "sign in"),
    ("log in to continue", "sign in"),
    ("login to continue", "sign in"),
    ("please sign in", "sign in"),
];

/// What a person must do before a task can go on, when the visible text
/// shows a captcha, a one-time code, two-factor authentication, or a login
/// wall; `None` otherwise.
///
/// ```
/// use tinycomputer_core::human_needed;
///
/// let page = ["Security check".to_owned(), "Verify you are human".to_owned()];
/// assert_eq!(human_needed(&page).as_deref(), Some("prove you are human"));
/// assert_eq!(human_needed(&["Flights from Delhi".to_owned()]), None);
/// ```
#[must_use]
pub fn human_needed(texts: &[String]) -> Option<String> {
    let words = normalize(&texts.join(" "));
    HUMAN_GATES
        .iter()
        .find(|(phrase, _)| has_phrase(&words, phrase))
        .map(|(_, action)| (*action).to_owned())
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
mod safety_tests;
