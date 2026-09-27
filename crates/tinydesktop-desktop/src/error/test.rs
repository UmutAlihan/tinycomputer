//! Unit tests for the crate-wide error type.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::Error;

#[test]
fn a_non_object_configuration_names_what_was_expected() {
    assert_eq!(
        Error::ConfigNotAnObject.to_string(),
        "module configuration must be a json object"
    );
}

#[test]
fn a_field_type_error_names_the_field_and_the_expected_type() {
    let error = Error::ConfigFieldType {
        field: "session_id",
        expected: "a string",
    };

    assert_eq!(
        error.to_string(),
        "module configuration field `session_id` must be a string"
    );
}

#[test]
fn a_context_failure_carries_the_engine_message() {
    let error = Error::Context("state root is not writable".to_owned());

    assert_eq!(
        error.to_string(),
        "cannot establish a desktop command context: state root is not writable"
    );
}
