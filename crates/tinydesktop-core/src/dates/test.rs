//! Tests for reading dates and retyping them in a form's layout.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{Date, date_pattern, parse_date, reformat_date};

const BIRTHDAY: Date = Date {
    year: 2000,
    month: 1,
    day: 31,
};

#[test]
fn unambiguous_dates_are_read_and_others_are_not() {
    for text in [
        "2000-01-31",
        "2000/1/31",
        "31 January 2000",
        "31 Jan 2000",
        "January 31, 2000",
        "Jan 31 2000",
    ] {
        assert_eq!(parse_date(text), Some(BIRTHDAY), "{text}");
    }
    for text in [
        "01/02/2000",
        "2000-13-01",
        "2000-01-32",
        "31 Ju 2000",
        "Asha",
        "",
        "0099-01-01",
        "31 Blah 2000",
    ] {
        assert_eq!(parse_date(text), None, "{text}");
    }
}

#[test]
fn the_layout_a_form_names_is_found_in_any_hint() {
    assert_eq!(
        date_pattern(["Date of Birth", "enter it as dd/mm/yyyy"]),
        Some("DD/MM/YYYY")
    );
    assert_eq!(date_pattern(["MM/DD/YY please"]), Some("MM/DD/YY"));
    assert_eq!(date_pattern(["Date of Birth"]), None);
}

#[test]
fn a_date_is_retyped_in_the_layout_asked_for() {
    for (hint, typed) in [
        ("DD-MM-YYYY", "31-01-2000"),
        ("MM/DD/YYYY", "01/31/2000"),
        ("YYYY-MM-DD", "2000-01-31"),
        ("DD/MM/YY", "31/01/00"),
        ("DD.MM.YYYY", "31.01.2000"),
    ] {
        assert_eq!(
            reformat_date("31 January 2000", [hint]).as_deref(),
            Some(typed),
            "{hint}"
        );
    }
    assert_eq!(reformat_date("2000-01-31", ["no layout here"]), None);
}
