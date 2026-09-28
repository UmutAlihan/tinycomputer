//! Tests for the `task_live` binary: how long each await waits.

use std::time::Duration;

use super::follow::{AWAIT_SLICE, next_wait};

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
