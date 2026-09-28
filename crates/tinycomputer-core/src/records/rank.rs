//! What "best" means when picking from records, and ranking by it.

use super::Record;
use super::{parse_clock, parse_duration, parse_price, parse_stops};

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
        if has(&[
            "cheapest",
            "lowest price",
            "least expensive",
            "lowest fare",
            "cheaper",
        ]) {
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
        // A named field is read first; failing that, any field that parses.
        // Prices found by scanning must show a currency, so a flight number
        // such as `6E-2135` is never mistaken for one.
        let named_or_any = |hints: &[&str], parse: &dyn Fn(&str) -> Option<f64>| {
            record
                .field(hints)
                .and_then(parse)
                .or_else(|| record.fields.values().find_map(|text| parse(text)))
        };
        match self {
            Self::LowestPrice | Self::HighestPrice => record
                .field(&["price", "fare", "cost", "total"])
                .and_then(parse_price)
                .or_else(|| {
                    record
                        .fields
                        .values()
                        .filter_map(|text| parse_price(text))
                        .find(|price| price.currency.is_some())
                })
                .map(|price| price.amount),
            Self::Earliest | Self::Latest => named_or_any(&["depart", "time", "start"], &|text| {
                parse_clock(text).map(f64::from)
            }),
            Self::FewestStops => named_or_any(&["stop"], &|text| parse_stops(text).map(f64::from)),
            Self::Shortest => named_or_any(&["duration", "length"], &|text| {
                parse_duration(text).map(f64::from)
            }),
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
/// use tinycomputer_core::{Criterion, Record, rank};
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
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        }
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    Some(order)
}
