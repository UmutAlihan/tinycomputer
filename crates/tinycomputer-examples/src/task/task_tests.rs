//! Tests for the task follower's pure parts: pacing, and what counts as a
//! pass.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::time::Duration;

use tinycomputer_bus::agent::TaskStatus;

use super::{AWAIT_SLICE, inputs_for, loggable, next_wait, passed, state};
use tinycomputer_bus::agent::{InputField, InputKind};

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

fn field(name: &str) -> InputField {
    InputField {
        name: name.to_owned(),
        why: String::new(),
        kind: InputKind::Text,
        options: Vec::new(),
    }
}

#[test]
fn a_pause_is_answered_only_when_every_field_is_known() {
    let answers = BTreeMap::from([("phone".to_owned(), "+91".to_owned())]);
    assert_eq!(
        inputs_for(&[field("phone")], &answers),
        Some(answers.clone())
    );
    assert_eq!(
        inputs_for(&[field("phone"), field("email")], &answers),
        None
    );
}

#[test]
fn a_pause_asking_for_nothing_is_handed_back() {
    let answers = BTreeMap::from([("phone".to_owned(), "+91".to_owned())]);
    assert_eq!(inputs_for(&[], &answers), None);
}

#[test]
fn a_logged_url_keeps_only_its_origin_and_path() {
    assert_eq!(
        loggable("https://user:secret@pay.test/checkout?token=abc#step"),
        "https://pay.test/checkout"
    );
    assert_eq!(loggable("about:blank"), "about:blank");
    // An `@` in the path is not a credential separator.
    assert_eq!(
        loggable("https://example.com/@alice?tab=1"),
        "https://example.com/@alice"
    );
    assert_eq!(loggable("https://u:p@example.com"), "https://example.com");
}
