//! Tests for the error envelopes the controller answers a call with.

use super::*;
use crate::task::errors::poisoned;

#[test]
fn a_poisoned_store_is_an_internal_error_a_caller_cannot_retry_past() {
    let reply: AgentResponse<TaskView> = poisoned();
    assert!(!reply.ok);
    assert!(reply.data.is_none());
    let error = reply.error.expect("a poisoned store is an error");
    assert_eq!(error.code, "INTERNAL");
    assert!(error.message.contains("poisoned"));
    assert_eq!(error.hint, "restart the module");
    assert!(!error.recoverable);
}
