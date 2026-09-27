//! Tests for the crate-wide error type.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::Error;

#[test]
fn a_refusal_names_the_fact_but_never_its_value() {
    let error = Error::CardDataRefused {
        name: "card".to_owned(),
    };
    assert_eq!(
        error.to_string(),
        "fact `card` looks like payment card data, which tasks never hold"
    );
}
