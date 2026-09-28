//! Tests for the crate-wide error type.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::Error;

#[test]
fn an_unknown_secret_names_the_secret() {
    let error = Error::UnknownSecret {
        name: "passport".to_owned(),
    };
    assert_eq!(
        error.to_string(),
        "`passport` is marked secret, but no fact is called that"
    );
}
