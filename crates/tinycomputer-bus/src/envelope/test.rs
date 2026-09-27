//! Unit tests pinning the response envelope's wire form.
//!
//! The envelope is the one payload both halves of the system parse, so its
//! field names are the contract. These tests fail if a rename would silently
//! turn a structured error into an absent one.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    Delivery, DeliveryDisposition, DesktopError, DesktopResponse, ENVELOPE_VERSION, RecoveryHint,
    RetryDisposition,
};
use serde_json::json;

#[test]
fn the_envelope_version_matches_the_engine() {
    assert_eq!(ENVELOPE_VERSION, "2.3");
}

#[test]
fn a_successful_reply_serializes_without_an_error_key() {
    let value = serde_json::to_value(DesktopResponse::ok("list-apps", json!({ "apps": [] })))
        .expect("a response serializes");

    assert_eq!(
        value,
        json!({
            "version": "2.3",
            "ok": true,
            "command": "list-apps",
            "data": { "apps": [] },
        })
    );
}

#[test]
fn a_failed_reply_serializes_without_a_data_key() {
    let value = serde_json::to_value(DesktopResponse::err(
        "click",
        DesktopError::new("STALE_REF", "ref is no longer valid")
            .with_suggestion("take a fresh snapshot"),
    ))
    .expect("a response serializes");

    assert_eq!(
        value,
        json!({
            "version": "2.3",
            "ok": false,
            "command": "click",
            "error": {
                "code": "STALE_REF",
                "message": "ref is no longer valid",
                "suggestion": "take a fresh snapshot",
                "disposition": { "delivery": "unknown", "retry": "unknown" },
            },
        })
    );
}

#[test]
fn an_engine_reply_round_trips_through_the_envelope() {
    // Verbatim output of `agent-desktop click @s1:e2 --json` on a stale ref,
    // which is what a host actually has to parse.
    let wire = json!({
        "version": "2.3",
        "ok": false,
        "command": "click",
        "error": {
            "code": "STALE_REF",
            "message": "Ref @s1:e2 is no longer valid",
            "suggestion": "Take a fresh snapshot",
            "recovery": {
                "strategy": "refresh_snapshot_then_retry_original",
                "retryable": true,
                "requires_fresh_snapshot": true,
            },
            "disposition": { "delivery": "not_delivered", "retry": "safe" },
        },
    });

    let reply: DesktopResponse = serde_json::from_value(wire.clone()).expect("the reply decodes");
    let error = reply
        .error
        .as_ref()
        .expect("a failed reply carries an error");

    assert!(!reply.ok);
    assert_eq!(error.code, "STALE_REF");
    assert_eq!(
        error
            .recovery
            .as_ref()
            .map(|hint| hint.requires_fresh_snapshot),
        Some(true)
    );
    assert_eq!(error.disposition.retry, RetryDisposition::Safe);
    assert_eq!(serde_json::to_value(&reply).expect("it re-encodes"), wire);
}

#[test]
fn a_recovery_hint_omits_an_absent_retry_delay() {
    let value = serde_json::to_value(RecoveryHint {
        strategy: "refresh_snapshot_then_retry_original".to_owned(),
        retryable: true,
        requires_fresh_snapshot: true,
        retry_after_ms: None,
    })
    .expect("a hint serializes");

    assert!(value.get("retry_after_ms").is_none());
}

#[test]
fn a_delivery_pair_follows_from_its_disposition() {
    assert_eq!(
        Delivery::of(DeliveryDisposition::NotDelivered).retry,
        RetryDisposition::Safe
    );
    for delivered in [
        DeliveryDisposition::DeliveryUncertain,
        DeliveryDisposition::DeliveredUnverified,
        DeliveryDisposition::DeliveredVerified,
    ] {
        assert_eq!(Delivery::of(delivered).retry, RetryDisposition::Unsafe);
    }
    assert_eq!(
        Delivery::of(DeliveryDisposition::Unknown).retry,
        RetryDisposition::Unknown
    );
}

#[test]
fn an_absent_disposition_decodes_as_unknown() {
    let error: DesktopError =
        serde_json::from_value(json!({ "code": "INTERNAL", "message": "boom" }))
            .expect("the error decodes");

    assert_eq!(error.disposition, Delivery::default());
    assert_eq!(error.disposition.delivery, DeliveryDisposition::Unknown);
}
