//! Whether a page is a payment step, from its fields, URL, and controls.

use super::{Consequence, consequence};
use super::{has_phrase, normalize};
use crate::surface::{Candidate, Screen};

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
