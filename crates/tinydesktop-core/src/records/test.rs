//! Tests for reading values off result cards and ranking them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    Criterion, Price, Record, parse_clock, parse_duration, parse_price, parse_stops, rank,
};

fn price(text: &str) -> (f64, Option<&'static str>) {
    let Price { amount, currency } = parse_price(text).unwrap_or_else(|| panic!("{text}"));
    (amount, currency)
}

#[test]
fn prices_read_symbols_codes_and_separators() {
    assert_eq!(price("₹6,840"), (6840.0, Some("INR")));
    assert_eq!(price("Rs. 1,23,456"), (123_456.0, Some("INR")));
    assert_eq!(price("INR 7,210"), (7210.0, Some("INR")));
    assert_eq!(price("$1,234.56"), (1234.56, Some("USD")));
    assert_eq!(price("US$ 99"), (99.0, Some("USD")));
    assert_eq!(price("1.234,50 €"), (1234.5, Some("EUR")));
    assert_eq!(price("£12,50"), (12.5, Some("GBP")));
    assert_eq!(price("¥12.000.000"), (12_000_000.0, Some("JPY")));
    assert_eq!(price("AED 450."), (450.0, Some("AED")));
    assert_eq!(price("6840"), (6840.0, None));
    assert_eq!(price("Total 6,840"), (6840.0, None));
}

#[test]
fn a_price_is_read_next_to_its_currency_not_from_a_flight_number() {
    assert_eq!(price("IndiGo 6E-2135 · ₹6,840"), (6840.0, Some("INR")));
    assert_eq!(price("4 hours · $300"), (300.0, Some("USD")));
    assert!(parse_price("Rs with nothing").is_none());
    assert!(parse_price("no digits here").is_none());
    assert!(parse_price("€").is_none());
}

#[test]
fn a_written_out_currency_reads_the_amount_before_it() {
    let card = "From 7339 Indian rupees. 1 stop flight with IndiGo. Leaves at 8:00 AM";
    assert_eq!(price(card), (7339.0, Some("INR")));
    assert_eq!(price("12,450 rupees"), (12450.0, Some("INR")));
    assert_eq!(price("about 300 US dollars"), (300.0, Some("USD")));
    let unpriced = "Total price is unavailable. Nonstop flight. Leaves at 9:55 AM";
    assert_eq!(price(unpriced), (9.0, None), "no currency, so never ranked");
    assert_eq!(price("rupees 5 later"), (5.0, None), "the amount must come first");
}

#[test]
fn cards_priced_in_words_rank_by_price() {
    let cards = [
        "From 8588 Indian rupees. Nonstop flight with Air India.",
        "Total price is unavailable. Nonstop flight with Air India Express. Leaves at 9:55 AM",
        "From 7339 Indian rupees. 1 stop flight with IndiGo.",
    ];
    let records = cards
        .iter()
        .map(|text| Record::from_pairs([("field 0", *text)]))
        .collect::<Vec<_>>();
    assert_eq!(rank(&records, Criterion::LowestPrice), Some(vec![2, 0, 1]));
}

#[test]
fn clock_times_read_twelve_and_twenty_four_hour_forms() {
    assert_eq!(parse_clock("06:45"), Some(6 * 60 + 45));
    assert_eq!(parse_clock("Departs 6:45 PM"), Some(18 * 60 + 45));
    assert_eq!(parse_clock("12:10 pm"), Some(12 * 60 + 10));
    assert_eq!(parse_clock("12:05 a.m."), Some(5));
    assert_eq!(parse_clock("23:59"), Some(23 * 60 + 59));
    assert_eq!(parse_clock("24:10"), None);
    assert_eq!(parse_clock("10:75"), None);
    assert_eq!(parse_clock("no time"), None);
    assert_eq!(parse_clock(":30"), None);
}

#[test]
fn durations_need_a_unit_word() {
    assert_eq!(parse_duration("2h 15m"), Some(135));
    assert_eq!(parse_duration("2 hr 15 min"), Some(135));
    assert_eq!(parse_duration("135 min"), Some(135));
    assert_eq!(parse_duration("1 hour"), Some(60));
    assert_eq!(parse_duration("15 May"), None);
    assert_eq!(parse_duration("1 stop"), None);
    assert_eq!(parse_duration("nothing"), None);
}

#[test]
fn stops_read_words_and_counts() {
    assert_eq!(parse_stops("Nonstop"), Some(0));
    assert_eq!(parse_stops("Non-stop"), Some(0));
    assert_eq!(parse_stops("Direct flight"), Some(0));
    assert_eq!(parse_stops("1 stop · DEL"), Some(1));
    assert_eq!(parse_stops("2 stops"), Some(2));
    assert_eq!(parse_stops("stops vary"), None);
    assert_eq!(parse_stops("economy"), None);
}

#[test]
fn criteria_are_read_from_plain_words() {
    for (text, expected) in [
        ("the cheapest flight", Some(Criterion::LowestPrice)),
        ("lowest price", Some(Criterion::LowestPrice)),
        ("most expensive room", Some(Criterion::HighestPrice)),
        ("fewest stops", Some(Criterion::FewestStops)),
        ("a direct flight", Some(Criterion::FewestStops)),
        ("the fastest", Some(Criterion::Shortest)),
        ("earliest departure", Some(Criterion::Earliest)),
        ("the latest one", Some(Criterion::Latest)),
        ("a morning flight with good reviews", None),
    ] {
        assert_eq!(Criterion::parse(text), expected, "{text}");
    }
}

fn flights() -> Vec<Record> {
    vec![
        Record::from_pairs([
            ("airline", "Vistara UK-707"),
            ("fare", "₹7,210"),
            ("departure", "09:10"),
            ("stops", "Nonstop"),
            ("duration", "1h 35m"),
        ]),
        Record::from_pairs([
            ("airline", "IndiGo 6E-2135"),
            ("fare", "₹6,840"),
            ("departure", "6:45 PM"),
            ("stops", "1 stop"),
            ("duration", "4h 10m"),
        ]),
        Record::from_pairs([("airline", "Unknown carrier")]),
        Record::from_pairs([
            ("airline", "Air India"),
            ("fare", "₹8,050"),
            ("departure", "05:30"),
            ("stops", "1 stop"),
            ("duration", "3h"),
        ]),
    ]
}

#[test]
fn ranking_orders_by_each_criterion_with_unreadable_records_last() {
    let flights = flights();
    assert_eq!(
        rank(&flights, Criterion::LowestPrice),
        Some(vec![1, 0, 3, 2])
    );
    assert_eq!(
        rank(&flights, Criterion::HighestPrice),
        Some(vec![3, 0, 1, 2])
    );
    assert_eq!(rank(&flights, Criterion::Earliest), Some(vec![3, 0, 1, 2]));
    assert_eq!(rank(&flights, Criterion::Latest), Some(vec![1, 0, 3, 2]));
    assert_eq!(
        rank(&flights, Criterion::FewestStops),
        Some(vec![0, 1, 3, 2])
    );
    assert_eq!(rank(&flights, Criterion::Shortest), Some(vec![0, 3, 1, 2]));
}

#[test]
fn unnamed_fields_are_scanned_but_a_price_must_show_its_currency() {
    let unnamed = [
        Record::from_pairs([("summary", "IndiGo 6E-2135, 6:45 PM, ₹6,840")]),
        Record::from_pairs([("summary", "Vistara UK-707, 09:10, ₹7,210")]),
    ];
    assert_eq!(rank(&unnamed, Criterion::LowestPrice), Some(vec![0, 1]));
    assert_eq!(rank(&unnamed, Criterion::Earliest), Some(vec![1, 0]));
    let no_currency = [Record::from_pairs([("flight", "6E-2135")])];
    assert_eq!(rank(&no_currency, Criterion::LowestPrice), None);
}

#[test]
fn nothing_readable_means_judgement_is_needed() {
    let records = [
        Record::from_pairs([("name", "Houseboat A")]),
        Record::default(),
    ];
    assert_eq!(rank(&records, Criterion::LowestPrice), None);
    assert_eq!(rank(&[], Criterion::Earliest), None);
}
