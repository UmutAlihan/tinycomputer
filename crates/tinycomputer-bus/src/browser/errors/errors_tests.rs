//! Tests for the wire error names.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    BLOCKED_BY_POLICY, BROWSER_UNAVAILABLE, INVALID_INPUT, LIMIT_EXCEEDED, MODULE_FAILED, NAMES,
    NO_SUCH_ELEMENT, NO_SUCH_OUTPUT, NO_SUCH_SESSION, NOT_ACTIONABLE, PAGE_ERROR, PREFIX,
    STALE_REF, TIMEOUT, code, is_agent_recoverable, recovery,
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
        "ai.tinyhumans.tinycomputer.Browser.Error.Invented"
    ));
}

#[test]
fn every_name_has_its_own_envelope_code() {
    let mut codes = NAMES.iter().map(|name| code(name)).collect::<Vec<_>>();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), NAMES.len(), "two names share a code");
}

#[test]
fn codes_reuse_the_desktop_spelling_where_the_meaning_matches() {
    assert_eq!(code(INVALID_INPUT), "INVALID_ARGS");
    assert_eq!(code(NO_SUCH_ELEMENT), "ELEMENT_NOT_FOUND");
    assert_eq!(code(STALE_REF), "STALE_REF");
    assert_eq!(code(TIMEOUT), "TIMEOUT");
    assert_eq!(code(BLOCKED_BY_POLICY), "POLICY_DENIED");
    assert_eq!(code(MODULE_FAILED), "INTERNAL");
    assert_eq!(code(NO_SUCH_SESSION), "SESSION_NOT_FOUND");
    assert_eq!(code(NO_SUCH_OUTPUT), "OUTPUT_NOT_FOUND");
}

#[test]
fn an_unknown_name_travels_as_internal() {
    assert_eq!(
        code("ai.tinyhumans.tinycomputer.Browser.Error.Invented"),
        "INTERNAL"
    );
}

#[test]
fn a_stale_ref_recovers_like_the_desktop_one() {
    let hint = recovery(STALE_REF).expect("a stale ref has a way out");
    assert_eq!(hint.strategy, "refresh_snapshot_then_retry_original");
    assert!(hint.retryable && hint.requires_fresh_snapshot);
}

#[test]
fn only_retryable_hints_are_offered_for_recoverable_names() {
    for name in NAMES {
        if recovery(name).is_some_and(|hint| hint.retryable) {
            assert!(
                is_agent_recoverable(name),
                "{name} is retryable but not recoverable"
            );
        }
    }
}

#[test]
fn a_refused_navigation_offers_no_way_out() {
    for name in [
        BLOCKED_BY_POLICY,
        BROWSER_UNAVAILABLE,
        LIMIT_EXCEEDED,
        MODULE_FAILED,
    ] {
        assert!(recovery(name).is_none(), "{name} must not suggest a retry");
    }
    assert!(recovery(NO_SUCH_SESSION).is_some_and(|hint| !hint.retryable));
}
