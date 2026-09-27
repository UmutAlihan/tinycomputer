//! Tests for the module's bus identity.
//!
//! These pin strings a host spells from this crate and a module answers to. A
//! change to one of them is a wire break, so it should have to be made twice —
//! once in the constant and once here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{INTERFACE, METHODS, OBJECT_PATH, methods};

#[test]
fn interface_is_the_published_name() {
    assert_eq!(INTERFACE, "ai.tinyhumans.tinybrowser.Browser");
}

#[test]
fn object_path_is_the_interface_in_path_form() {
    assert_eq!(OBJECT_PATH, "/ai/tinyhumans/tinybrowser/Browser");
    assert_eq!(OBJECT_PATH, format!("/{}", INTERFACE.replace('.', "/")));
}

#[test]
fn methods_lists_every_member_once() {
    let mut sorted = METHODS.to_vec();
    sorted.sort_unstable();
    let count = sorted.len();
    sorted.dedup();

    assert_eq!(sorted.len(), count, "METHODS contains a duplicate");
    assert_eq!(count, 14);
}

#[test]
fn method_constants_are_pascal_case_on_the_wire() {
    for member in METHODS {
        let first = member.chars().next().expect("member name is not empty");
        assert!(first.is_ascii_uppercase(), "{member} is not PascalCase");
        assert!(
            member.chars().all(|c| c.is_ascii_alphanumeric()),
            "{member} is not a bare identifier"
        );
    }
}

#[test]
fn every_member_constant_appears_in_methods() {
    for member in [
        methods::OPEN_SESSION,
        methods::CLOSE_SESSION,
        methods::LIST_SESSIONS,
        methods::NAVIGATE,
        methods::SNAPSHOT,
        methods::PERFORM,
        methods::READ_PAGE,
        methods::EVALUATE,
        methods::SCREENSHOT,
        methods::READ_OUTPUT,
        methods::RELEASE_OUTPUT,
        methods::LIST_DOWNLOADS,
        methods::WAIT_DOWNLOAD,
        methods::CONTRACT_VERSION,
    ] {
        assert!(
            METHODS.contains(&member),
            "{member} is missing from METHODS"
        );
    }
}
