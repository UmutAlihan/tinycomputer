//! Tests for the crate-wide error type.
//!
//! The point of these is the mapping: a variant whose wire name is wrong sends a
//! host down the wrong recovery path, and nothing else in the build checks it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::Error;
use tinybrowser_bus::errors;

#[test]
fn every_variant_maps_to_a_published_name() {
    for error in every_variant() {
        assert!(
            errors::NAMES.contains(&error.wire_name()),
            "{error} maps to {}, which the contract does not publish",
            error.wire_name()
        );
    }
}

#[test]
fn the_variants_a_model_can_fix_are_the_recoverable_ones() {
    assert!(errors::is_agent_recoverable(
        Error::invalid_input("bad").wire_name()
    ));
    assert!(errors::is_agent_recoverable(
        Error::not_actionable("covered").wire_name()
    ));
    assert!(errors::is_agent_recoverable(
        Error::timeout("navigate", 30_000).wire_name()
    ));
    assert!(errors::is_agent_recoverable(
        Error::NoSuchElement {
            target: "@e1".to_string()
        }
        .wire_name()
    ));
}

#[test]
fn deployment_problems_are_not_offered_back_to_the_model() {
    assert!(!errors::is_agent_recoverable(
        Error::browser_unavailable("no chrome").wire_name()
    ));
    assert!(!errors::is_agent_recoverable(
        Error::BlockedByPolicy {
            url: "https://evil.test/".to_string()
        }
        .wire_name()
    ));
}

#[test]
fn a_lost_connection_tells_the_host_to_open_a_new_session() {
    // Not `ModuleFailed`: the session is gone, and `NoSuchSession` is the name
    // that makes a host reopen rather than retry into a dead socket.
    assert_eq!(
        Error::connection_lost("websocket closed").wire_name(),
        errors::NO_SUCH_SESSION
    );
}

#[test]
fn stale_ref_says_which_snapshot_minted_it() {
    let error = Error::StaleRef {
        reference: "e12".to_string(),
        minted: 1,
        current: 3,
    };

    assert_eq!(error.wire_name(), errors::STALE_REF);
    assert_eq!(
        error.to_string(),
        "ref @e12 is from snapshot 1, the page is now at snapshot 3"
    );
}

#[test]
fn messages_are_lowercase_and_unpunctuated() {
    for error in every_variant() {
        let rendered = error.to_string();
        let first = rendered.chars().next().expect("a message");

        assert!(
            !first.is_uppercase(),
            "{rendered} starts with a capital letter"
        );
        assert!(!rendered.ends_with('.'), "{rendered} ends with a full stop");
    }
}

/// One of every variant, so the mapping tests cannot miss a new one.
fn every_variant() -> Vec<Error> {
    vec![
        Error::invalid_input("bad url"),
        Error::NoSuchSession {
            id: "s-1".to_string(),
        },
        Error::NoSuchElement {
            target: "@e1".to_string(),
        },
        Error::StaleRef {
            reference: "e1".to_string(),
            minted: 1,
            current: 2,
        },
        Error::not_actionable("covered by a banner"),
        Error::timeout("navigate", 30_000),
        Error::BlockedByPolicy {
            url: "https://evil.test/".to_string(),
        },
        Error::browser_unavailable("no chrome on this host"),
        Error::page("ReferenceError: x is not defined"),
        Error::NoSuchOutput {
            id: "o-1".to_string(),
        },
        Error::LimitExceeded {
            message: "too many sessions".to_string(),
        },
        Error::connection_lost("websocket closed"),
        Error::failed("something else"),
    ]
}
