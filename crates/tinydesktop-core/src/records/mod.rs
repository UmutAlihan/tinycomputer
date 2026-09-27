//! Records extracted from a page, the values in them, and ranking.
//!
//! A results page — flights, hotels, products — is a list of repeated cards.
//! Once each card is a [`Record`] of named text fields, picking "the cheapest"
//! or "the earliest" is arithmetic, not judgement: [`rank`] does it
//! deterministically whenever the criterion names something these parsers can
//! read, and a decision model is asked only when it cannot.

use std::collections::BTreeMap;

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

/// Reads the first price in `text`, such as `₹6,840`, `$1,234.56`, or
/// `1.234,50 €`.
///
/// Separators are resolved by position: when both `.` and `,` appear, the
/// later one is the decimal mark; a lone `,` followed by exactly two digits
/// is a decimal mark, and otherwise groups thousands (including the Indian
/// `1,23,456`). A lone `.` is a decimal mark.
///
/// ```
/// use tinydesktop_core::parse_price;
///
/// let price = parse_price("IndiGo · ₹6,840").unwrap();
/// assert_eq!((price.amount, price.currency), (6840.0, Some("INR")));
/// ```
#[must_use]
pub fn parse_price(text: &str) -> Option<Price> {
    let lower = text.to_lowercase();
    let currency = CURRENCIES
        .iter()
        .find(|(marker, _)| {
            lower.match_indices(marker).any(|(at, _)| {
                // A letter code must stand alone, not sit inside a word.
                marker.chars().all(char::is_alphabetic).then_some(()).map_or(true, |()| {
                    let before = lower[..at].chars().next_back();
                    let after = lower[at + marker.len()..].chars().next();
                    !before.is_some_and(char::is_alphabetic) && !after.is_some_and(char::is_alphabetic)
                })
            })
        })
        .map(|(_, code)| *code);
    let start = text.find(|character: char| character.is_ascii_digit())?;
    let number: String = text[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit() || matches!(character, ',' | '.'))
        .collect();
    let number = number.trim_end_matches([',', '.']);
    let amount = decimal(number)?;
    Some(Price { amount, currency })
}

fn decimal(number: &str) -> Option<f64> {
    let last_dot = number.rfind('.');
    let last_comma = number.rfind(',');
    let normalized = match (last_dot, last_comma) {
        (Some(dot), Some(comma)) if dot > comma => number.replace(',', ""),
        (Some(_), Some(_)) => number.replace('.', "").replace(',', "."),
        (None, Some(comma)) if number.len() - comma - 1 == 2 && number.matches(',').count() == 1 => {
            number.replace(',', ".")
        }
        (None, Some(_)) => number.replace(',', ""),
        (Some(_), None) if number.matches('.').count() > 1 => number.replace('.', ""),
        _ => number.to_owned(),
    };
    normalized.parse().ok()
}

/// Reads a clock time as minutes after midnight: `06:45`, `6:45 PM`, `18:05`.
///
/// ```
/// use tinydesktop_core::parse_clock;
///
/// assert_eq!(parse_clock("Departs 6:45 PM"), Some(18 * 60 + 45));
/// ```
#[must_use]
pub fn parse_clock(text: &str) -> Option<u32> {
    let lower = text.to_ascii_lowercase();
    let colon = lower.find(':')?;
    let hours: u32 = lower[..colon]
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>()
        .parse()
        .ok()?;
    let minutes: u32 = lower.get(colon + 1..colon + 3)?.parse().ok()?;
    if minutes > 59 {
        return None;
    }
    let rest = lower[colon + 3..].trim_start();
    let hours = match (rest.starts_with("pm") || rest.starts_with("p.m"), rest.starts_with("am") || rest.starts_with("a.m")) {
        (true, _) if hours < 12 => hours + 12,
        (_, true) if hours == 12 => 0,
        _ => hours,
    };
    (hours < 24).then_some(hours * 60 + minutes)
}

/// Reads a duration as minutes: `2h 15m`, `2 hr 15 min`, `135 min`, `1h`.
///
/// ```
/// use tinydesktop_core::parse_duration;
///
/// assert_eq!(parse_duration("2h 15m"), Some(135));
/// ```
#[must_use]
pub fn parse_duration(text: &str) -> Option<u32> {
    let lower = text.to_ascii_lowercase();
    let mut total = 0;
    let mut found = false;
    let mut digits = String::new();
    let mut chars = lower.chars().peekable();
    while let Some(character) = chars.next() {
        if character.is_ascii_digit() {
            digits.push(character);
            continue;
        }
        if digits.is_empty() || character == ' ' && chars.peek().is_some_and(char::is_ascii_digit) {
            continue;
        }
        if character == ' ' {
            continue;
        }
        let value: u32 = digits.parse().ok()?;
        digits.clear();
        match character {
            'h' => total += value * 60,
            'm' => total += value,
            _ => continue,
        }
        found = true;
    }
    found.then_some(total)
}

/// Reads how many stops a journey makes: `Nonstop` and `Direct` are zero.
///
/// ```
/// use tinydesktop_core::parse_stops;
///
/// assert_eq!(parse_stops("Non-stop"), Some(0));
/// assert_eq!(parse_stops("1 stop · DEL"), Some(1));
/// ```
#[must_use]
pub fn parse_stops(text: &str) -> Option<u32> {
    let lower = text.to_ascii_lowercase().replace('-', "");
    if lower.contains("nonstop") || lower.contains("direct") {
        return Some(0);
    }
    let at = lower.find("stop")?;
    lower[..at]
        .split_whitespace()
        .next_back()
        .and_then(|word| word.parse().ok())
}

/// What "best" means when picking from records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Criterion {
    /// The lowest price first.
    LowestPrice,
    /// The highest price first.
    HighestPrice,
    /// The earliest time first.
    Earliest,
    /// The latest time first.
    Latest,
    /// The fewest stops first.
    FewestStops,
    /// The shortest duration first.
    Shortest,
}

impl Criterion {
    /// Reads a criterion from plain words, such as `cheapest` or
    /// `lowest price`; `None` when the words need judgement instead.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let lower = text.to_ascii_lowercase();
        let has = |words: &[&str]| words.iter().any(|word| lower.contains(word));
        if has(&["cheapest", "lowest price", "least expensive", "lowest fare", "cheaper"]) {
            Some(Self::LowestPrice)
        } else if has(&["most expensive", "highest price"]) {
            Some(Self::HighestPrice)
        } else if has(&["fewest stops", "nonstop", "non-stop", "direct"]) {
            Some(Self::FewestStops)
        } else if has(&["shortest", "fastest", "quickest"]) {
            Some(Self::Shortest)
        } else if has(&["earliest", "first departure", "soonest"]) {
            Some(Self::Earliest)
        } else if has(&["latest", "last departure"]) {
            Some(Self::Latest)
        } else {
            None
        }
    }

    fn key(self, record: &Record) -> Option<f64> {
        let any_field = |parse: fn(&str) -> Option<f64>, hints: &[&str]| {
            record
                .field(hints)
                .and_then(parse)
                .or_else(|| record.fields.values().find_map(|text| parse(text)))
        };
        match self {
            Self::LowestPrice | Self::HighestPrice => any_field(
                |text| parse_price(text).map(|price| price.amount),
                &["price", "fare", "cost", "total"],
            ),
            Self::Earliest | Self::Latest => any_field(
                |text| parse_clock(text).map(f64::from),
                &["depart", "time", "start"],
            ),
            Self::FewestStops => {
                any_field(|text| parse_stops(text).map(f64::from), &["stop"])
            }
            Self::Shortest => any_field(
                |text| parse_duration(text).map(f64::from),
                &["duration", "length", "travel time"],
            ),
        }
    }
}

/// Record indexes, best first, for `criterion`.
///
/// Records whose value cannot be read go last, in their original order.
/// `None` when no record can be read at all — the cue to ask a decision
/// model instead.
///
/// ```
/// use tinydesktop_core::{Criterion, Record, rank};
///
/// let flights = [
///     Record::from_pairs([("airline", "Vistara"), ("price", "₹7,210")]),
///     Record::from_pairs([("airline", "IndiGo"), ("price", "₹6,840")]),
/// ];
/// assert_eq!(rank(&flights, Criterion::LowestPrice), Some(vec![1, 0]));
/// ```
#[must_use]
pub fn rank(records: &[Record], criterion: Criterion) -> Option<Vec<usize>> {
    let keyed = records
        .iter()
        .map(|record| criterion.key(record))
        .collect::<Vec<_>>();
    if keyed.iter().all(Option::is_none) {
        return None;
    }
    let descending = matches!(criterion, Criterion::HighestPrice | Criterion::Latest);
    let mut order = (0..records.len()).collect::<Vec<_>>();
    order.sort_by(|&left, &right| match (keyed[left], keyed[right]) {
        (Some(a), Some(b)) => {
            let ordering = a.total_cmp(&b);
            if descending { ordering.reverse() } else { ordering }
        }
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    Some(order)
}

#[cfg(test)]
mod test;
