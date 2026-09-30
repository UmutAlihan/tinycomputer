//! Unit tests for the Globe listener's bounded event queue and platform fallbacks.

#![allow(clippy::expect_used)]

use super::{MAX_PENDING_EVENTS, trim_event_queue};
use std::collections::VecDeque;

#[test]
fn event_queue_keeps_latest_events() {
    let mut queue = VecDeque::new();
    for index in 0..(MAX_PENDING_EVENTS + 5) {
        queue.push_back(format!("event-{index}"));
        trim_event_queue(&mut queue);
    }

    assert_eq!(queue.len(), MAX_PENDING_EVENTS);
    assert_eq!(queue.front().map(String::as_str), Some("event-5"));
    let expected_last = format!("event-{}", MAX_PENDING_EVENTS + 4);
    assert_eq!(
        queue.back().map(String::as_str),
        Some(expected_last.as_str())
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn non_macos_listener_entry_points_report_unsupported() {
    let started = super::globe_listener_start().expect("fallback start returns status");
    assert!(!started.supported);
    assert!(!started.running);
    assert_eq!(started.events_pending, 0);

    let polled = super::globe_listener_poll().expect("fallback poll returns status");
    assert!(!polled.status.supported);
    assert!(!polled.status.running);
    assert!(polled.events.is_empty());

    let stopped = super::globe_listener_stop().expect("fallback stop returns status");
    assert!(!stopped.supported);
    assert!(!stopped.running);
}
