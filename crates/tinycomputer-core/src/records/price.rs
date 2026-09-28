//! Reading a price as shown: its amount, its currency, and its separators.

/// A price as shown: an amount and, when shown, its currency.
#[derive(Debug, Clone, PartialEq)]
pub struct Price {
    /// The amount, in the currency's major unit.
    pub amount: f64,
    /// An ISO code when the text names or symbolizes one.
    pub currency: Option<&'static str>,
}

/// Currency markers, longest first so `Rs.` wins over `Rs`.
const CURRENCIES: &[(&str, &str)] = &[
    ("₹", "INR"),
    ("rs.", "INR"),
    ("rs", "INR"),
    ("inr", "INR"),
    ("us$", "USD"),
    ("usd", "USD"),
    ("$", "USD"),
    ("€", "EUR"),
    ("eur", "EUR"),
    ("£", "GBP"),
    ("gbp", "GBP"),
    ("¥", "JPY"),
    ("jpy", "JPY"),
    ("aed", "AED"),
];

/// Currency names written out, as screen readers hear prices (`From 7339
/// Indian rupees`); the amount always comes before them. Every name is listed
/// in the singular and the plural, and longest first, so of two names starting
/// at the same place the longer is read.
const NAMED_CURRENCIES: &[(&str, &str)] = &[
    ("indian rupees", "INR"),
    ("indian rupee", "INR"),
    ("rupees", "INR"),
    ("rupee", "INR"),
    ("us dollars", "USD"),
    ("us dollar", "USD"),
    ("dollars", "USD"),
    ("dollar", "USD"),
    ("euros", "EUR"),
    ("euro", "EUR"),
    ("british pounds", "GBP"),
    ("british pound", "GBP"),
    ("pounds sterling", "GBP"),
    ("pound sterling", "GBP"),
    ("pounds", "GBP"),
    ("pound", "GBP"),
    ("japanese yen", "JPY"),
    ("yen", "JPY"),
    ("dirhams", "AED"),
    ("dirham", "AED"),
];

/// Reads the first price in `text`, such as `₹6,840`, `$1,234.56`,
/// `1.234,50 €`, or `7339 Indian rupees`.
///
/// Separators are resolved by position: when both `.` and `,` appear, the
/// later one is the decimal mark; a lone `,` followed by exactly two digits
/// is a decimal mark, and otherwise groups thousands (including the Indian
/// `1,23,456`). A lone `.` is a decimal mark.
///
/// ```
/// use tinycomputer_core::parse_price;
///
/// let price = parse_price("IndiGo · ₹6,840").unwrap();
/// assert_eq!((price.amount, price.currency), (6840.0, Some("INR")));
/// ```
#[must_use]
pub fn parse_price(text: &str) -> Option<Price> {
    let lower = text.to_lowercase();
    let marked = CURRENCIES.iter().find_map(|(marker, code)| {
        lower
            .match_indices(marker)
            .find(|(at, _)| standalone(&lower, *at, marker))
            .map(|(at, _)| (at, at + marker.len(), *code))
    });
    // The earliest name in the text with an amount before it, whichever
    // currency it is: the first price, not the first currency listed.
    let named = || {
        NAMED_CURRENCIES
            .iter()
            .flat_map(|(name, code)| {
                lower
                    .match_indices(name)
                    .filter(|(at, _)| standalone(&lower, *at, name))
                    .filter_map(|(at, _)| Some((at, number_before(&text[..at])?, *code)))
            })
            .min_by_key(|(at, _, _)| *at)
            .map(|(_, number, code)| (number, code))
    };
    // The amount sits right after a marker (`₹6,840`), else right before it
    // (`1.234,50 €`); a written-out name always follows its amount.
    let (number, currency) = if let Some((start, end, code)) = marked {
        (
            number_after(&text[end..]).or_else(|| number_before(&text[..start]))?,
            Some(code),
        )
    } else if let Some((number, code)) = named() {
        (number, Some(code))
    } else {
        (
            digits(&text[text.find(|character: char| character.is_ascii_digit())?..]),
            None,
        )
    };
    Some(Price {
        amount: decimal(&number)?,
        currency,
    })
}

/// Whether a letter marker at `at` stands alone rather than inside a word.
fn standalone(text: &str, at: usize, marker: &str) -> bool {
    if !marker.chars().any(char::is_alphabetic) {
        return true;
    }
    let before = text[..at].chars().next_back();
    let after = text[at + marker.len()..].chars().next();
    !before.is_some_and(char::is_alphabetic) && !after.is_some_and(char::is_alphabetic)
}

/// The number that starts `text`, after any spaces or punctuation.
fn number_after(text: &str) -> Option<String> {
    let start = text.find(|character: char| character.is_ascii_digit())?;
    if text[..start].chars().any(char::is_alphanumeric) {
        return None;
    }
    Some(digits(&text[start..]))
}

/// The number that `text` ends with, ignoring trailing spaces.
fn number_before(text: &str) -> Option<String> {
    let trimmed = text.trim_end();
    let length = trimmed
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_digit() || matches!(character, ',' | '.'))
        .map(char::len_utf8)
        .sum::<usize>();
    let number = &trimmed[trimmed.len() - length..];
    number
        .starts_with(|character: char| character.is_ascii_digit())
        .then(|| digits(number))
}

fn digits(text: &str) -> String {
    text.chars()
        .take_while(|character| character.is_ascii_digit() || matches!(character, ',' | '.'))
        .collect::<String>()
        .trim_end_matches([',', '.'])
        .to_owned()
}

fn decimal(number: &str) -> Option<f64> {
    let last_dot = number.rfind('.');
    let last_comma = number.rfind(',');
    let normalized = match (last_dot, last_comma) {
        (Some(dot), Some(comma)) if dot > comma => number.replace(',', ""),
        (Some(_), Some(_)) => number.replace('.', "").replace(',', "."),
        (None, Some(comma))
            if number.len() - comma - 1 == 2 && number.matches(',').count() == 1 =>
        {
            number.replace(',', ".")
        }
        (None, Some(_)) => number.replace(',', ""),
        (Some(_), None) if number.matches('.').count() > 1 => number.replace('.', ""),
        _ => number.to_owned(),
    };
    normalized.parse().ok()
}
