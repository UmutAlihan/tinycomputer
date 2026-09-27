//! Tests pinning the Agent interface's identity and member list.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{CONFIDENTIAL, INTERFACE, METHODS, OBJECT_PATH};

#[test]
fn the_members_are_served_on_the_modules_one_interface() {
    assert_eq!(INTERFACE, crate::names::INTERFACE);
    assert_eq!(OBJECT_PATH, crate::names::OBJECT_PATH);
    assert!(
        METHODS
            .iter()
            .all(|member| crate::names::METHODS.contains(member))
    );
}

#[test]
fn members_are_unique_and_confidential_ones_are_members() {
    assert_eq!(
        METHODS,
        [
            "Describe",
            "PlanTask",
            "StartTask",
            "AwaitTask",
            "ContinueTask",
            "CancelTask",
            "TaskReport",
            "ListTasks"
        ]
    );
    let mut unique = METHODS.to_vec();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), METHODS.len());
    assert!(CONFIDENTIAL.iter().all(|member| METHODS.contains(member)));
}
