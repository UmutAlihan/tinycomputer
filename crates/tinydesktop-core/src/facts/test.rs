//! Tests for the facts store: lookup, redaction, and refusing card data.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::Facts;
use crate::error::Error;

fn traveller() -> Facts {
    Facts::new([
        ("first name", "Asha"),
        ("last name", "Raina"),
        ("email", "asha@example.com"),
        ("phone", "+91 98765 43210"),
        ("empty", ""),
    ])
    .unwrap()
}

#[test]
fn values_are_looked_up_by_name_and_only_names_are_listed() {
    let facts = traveller();
    assert_eq!(facts.get("email"), Some("asha@example.com"));
    assert_eq!(facts.get("passport"), None);
    assert_eq!(
        facts.names(),
        ["email", "empty", "first name", "last name", "phone"]
    );
    let rendered = format!("{facts:?}");
    assert!(
        rendered.contains("email") && !rendered.contains("asha@"),
        "{rendered}"
    );
}

#[test]
fn missing_names_are_reported_in_order() {
    let wanted = ["passport", "email", "date of birth"].map(str::to_owned);
    assert_eq!(traveller().missing(&wanted), ["passport", "date of birth"]);
}

#[test]
fn redaction_replaces_values_with_their_names() {
    let text = "Booked for Asha Raina (asha@example.com), call +91 98765 43210.";
    assert_eq!(
        traveller().redact(text),
        "Booked for ‹first name› ‹last name› (‹email›), call ‹phone›."
    );
    let nested = Facts::new([("short", "Ann"), ("long", "Annabel")]).unwrap();
    assert_eq!(nested.redact("Annabel and Ann"), "‹long› and ‹short›");
}

#[test]
fn card_data_is_refused_by_name_and_by_value() {
    for (name, value) in [
        ("Card Number", "anything"),
        ("cvv", "123"),
        ("security_code", "999"),
        ("notes", "4111 1111 1111 1111"),
        ("reference", "4111-1111-1111-1111"),
    ] {
        assert_eq!(
            Facts::new([(name, value)]),
            Err(Error::CardDataRefused {
                name: name.to_owned()
            }),
            "{name}"
        );
    }
}

#[test]
fn ordinary_numbers_are_not_mistaken_for_cards() {
    for value in [
        "4111 1111 1111 1112",
        "98765 43210",
        "A1234567",
        "12345678901234567890",
        "2026-10-14",
    ] {
        assert!(Facts::new([("reference", value)]).is_ok(), "{value}");
    }
}
