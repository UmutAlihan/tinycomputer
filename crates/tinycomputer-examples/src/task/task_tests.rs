//! Tests for the task follower's pure parts: pacing, and what counts as a
//! pass.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::time::Duration;

use tinycomputer_bus::agent::TaskStatus;

use super::{AWAIT_SLICE, next_wait, passed, state};

const LIMIT: Duration = Duration::from_secs(20 * 60);

#[test]
fn waits_a_full_slice_while_plenty_of_time_remains() {
    assert_eq!(next_wait(Duration::ZERO, LIMIT), Some(AWAIT_SLICE));
}

#[test]
fn waits_only_the_time_left_near_the_limit() {
    let elapsed = Duration::from_secs(20 * 60 - 10);
    assert_eq!(next_wait(elapsed, LIMIT), Some(Duration::from_secs(10)));
}

#[test]
fn stops_waiting_once_the_limit_is_spent() {
    assert_eq!(next_wait(LIMIT, LIMIT), None);
    assert_eq!(next_wait(Duration::from_secs(20 * 60 + 1), LIMIT), None);
}

#[test]
fn a_finished_task_or_a_payment_stop_passes() {
    assert!(passed(&TaskStatus::Done {
        answer: String::new(),
        records: BTreeMap::new(),
        result: None,
    }));
    let checkpoint = |reason: &str| TaskStatus::Checkpoint {
        reason: reason.to_owned(),
        location: String::new(),
        screenshot: None,
        summary: String::new(),
        continuable: false,
    };
    assert!(passed(&checkpoint("reached the payment page")));
    assert!(!passed(&checkpoint("a login wall")));
    assert!(!passed(&TaskStatus::Cancelled));
}

#[test]
fn a_status_is_named_by_its_wire_state() {
    assert_eq!(state(&TaskStatus::Running), "running");
    assert_eq!(state(&TaskStatus::Cancelled), "cancelled");
}
