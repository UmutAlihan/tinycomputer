//! Turning an engine result into the response envelope.
//!
//! The mapping is deliberately lossless in the direction that matters. An
//! `AppError::Adapter` already carries a code, a platform detail, structured
//! details, and a delivery disposition; those are copied across rather than
//! rendered into the message, because the whole reason the envelope has fields
//! for them is that a caller acts on them. An error of any other shape has only
//! a message and a code, so that is what it becomes.

use agent_desktop_core::{AppError, DeliverySemantics, RetryDisposition, output::ErrorPayload};
use serde_json::Value;
use tinydesktop_bus::{
    Delivery, DeliveryDisposition, DesktopError, DesktopResponse, RecoveryHint,
    RetryDisposition as ContractRetry,
};

/// Wraps `result` in the envelope, naming it `command`.
pub(super) fn envelope(command: &str, result: Result<Value, AppError>) -> DesktopResponse {
    match result {
        Ok(data) => DesktopResponse::ok(command, data),
        Err(error) => DesktopResponse::err(command, desktop_error(&error)),
    }
}

/// Converts an engine error into the contract's error payload.
///
/// The engine's own [`ErrorPayload`] does this work already — including
/// choosing the recovery strategy for a retry-safe code — so this reuses it
/// rather than reimplementing the mapping and drifting from it.
fn desktop_error(error: &AppError) -> DesktopError {
    let payload = ErrorPayload::from_app_error(error);

    DesktopError {
        code: payload.code,
        message: payload.message,
        suggestion: payload.suggestion,
        recovery: payload.recovery.map(|hint| RecoveryHint {
            strategy: hint.strategy,
            retryable: hint.retryable,
            requires_fresh_snapshot: hint.requires_fresh_snapshot,
            retry_after_ms: hint.retry_after_ms,
        }),
        platform_detail: payload.platform_detail,
        details: payload.details,
        disposition: delivery(payload.disposition),
    }
}

/// Converts the engine's delivery semantics into the contract's pair.
fn delivery(semantics: DeliverySemantics) -> Delivery {
    let disposition = match semantics {
        DeliverySemantics::Unknown => DeliveryDisposition::Unknown,
        DeliverySemantics::NotDelivered => DeliveryDisposition::NotDelivered,
        DeliverySemantics::DeliveryUncertain => DeliveryDisposition::DeliveryUncertain,
        DeliverySemantics::DeliveredUnverified => DeliveryDisposition::DeliveredUnverified,
        DeliverySemantics::DeliveredVerified => DeliveryDisposition::DeliveredVerified,
    };
    let retry = match semantics.retry() {
        RetryDisposition::Unknown => ContractRetry::Unknown,
        RetryDisposition::Safe => ContractRetry::Safe,
        RetryDisposition::Unsafe => ContractRetry::Unsafe,
    };

    Delivery {
        delivery: disposition,
        retry,
    }
}
