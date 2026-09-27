//! Tests for the wire error names.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    BLOCKED_BY_POLICY, BROWSER_UNAVAILABLE, INVALID_INPUT, LIMIT_EXCEEDED, MODULE_FAILED, NAMES,
    NO_SUCH_ELEMENT, NO_SUCH_OUTPUT, NO_SUCH_SESSION, NOT_ACTIONABLE, PAGE_ERROR, PREFIX,
    STALE_REF, TIMEOUT, is_agent_recoverable,
};

#[test]
fn every_name_sits_under_the_prefix() {
    for name in NAMES {
        assert!(
            name.starts_with(PREFIX),
            "{name} is outside {PREFIX}, so a host matching on the prefix would miss it"
        );
    }
}

#[test]
fn names_lists_every_variant_once() {
    let mut sorted = NAMES.to_vec();
    sorted.sort_unstable();
    let count = sorted.len();
    sorted.dedup();

    assert_eq!(sorted.len(), count, "NAMES contains a duplicate");
    assert_eq!(count, 12);
}

#[test]
fn recoverable_names_are_the_ones_a_model_can_act_on() {
    for name in [
        INVALID_INPUT,
        NO_SUCH_ELEMENT,
        STALE_REF,
        NOT_ACTIONABLE,
        TIMEOUT,
        PAGE_ERROR,
    ] {
        assert!(is_agent_recoverable(name), "{name} should be recoverable");
    }
}

#[test]
fn operator_problems_are_not_offered_back_to_the_model() {
    for name in [
        NO_SUCH_SESSION,
        BLOCKED_BY_POLICY,
        BROWSER_UNAVAILABLE,
        NO_SUCH_OUTPUT,
        LIMIT_EXCEEDED,
        MODULE_FAILED,
    ] {
        assert!(
            !is_agent_recoverable(name),
            "{name} should not be offered back to the model"
        );
    }
}

#[test]
fn an_unknown_name_is_not_recoverable() {
    // A module from a newer contract can send a name this build has never seen.
    // Treating it as recoverable would have an agent retry something it cannot
    // understand; treating it as an operator problem surfaces it instead.
    assert!(!is_agent_recoverable(
        "ai.tinyhumans.tinybrowser.Error.Invented"
    ));
}
