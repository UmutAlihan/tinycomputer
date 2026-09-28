//! Tests for the member catalogue: it names exactly the served members, in
//! order, and its families and flags agree with the per-family name lists.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{Family, MEMBERS, member, summaries};
use serde_json::json;

#[test]
fn the_catalogue_lists_exactly_the_served_members_in_order() {
    let names = MEMBERS.iter().map(|member| member.name).collect::<Vec<_>>();
    assert_eq!(names, crate::names::METHODS.to_vec());
}

#[test]
fn every_task_member_is_in_the_task_family_and_nothing_else_is() {
    for entry in MEMBERS {
        assert_eq!(
            entry.family == Family::Task,
            crate::agent::names::METHODS.contains(&entry.name),
            "{} is in the wrong family",
            entry.name
        );
    }
}

#[test]
fn every_browser_member_is_in_the_browser_family_and_nothing_else_is() {
    for entry in MEMBERS {
        assert_eq!(
            entry.family == Family::Browser,
            crate::browser::names::METHODS.contains(&entry.name),
            "{} is in the wrong family",
            entry.name
        );
    }
}

#[test]
fn task_confidentiality_agrees_with_the_task_names() {
    for entry in MEMBERS.iter().filter(|entry| entry.family == Family::Task) {
        assert_eq!(
            entry.confidential,
            crate::agent::names::CONFIDENTIAL.contains(&entry.name),
            "{} disagrees with agent::names::CONFIDENTIAL",
            entry.name
        );
    }
}

#[test]
fn every_summary_is_one_sentence() {
    for entry in MEMBERS {
        assert!(entry.summary.ends_with('.'), "{} summary", entry.name);
        assert!(entry.summary.len() < 120, "{} summary is long", entry.name);
    }
}

#[test]
fn an_unknown_member_has_no_entry() {
    assert!(member("Teleport").is_none());
    assert_eq!(member("BrowserNavigate").map(|m| m.family), Some(Family::Browser));
}

#[test]
fn a_summary_has_a_pinned_wire_form() {
    let summary = summaries()
        .into_iter()
        .find(|summary| summary.name == "Snapshot")
        .expect("Snapshot is catalogued");
    assert_eq!(
        serde_json::to_value(summary).expect("serializes"),
        json!({
            "name": "Snapshot",
            "family": "desktop",
            "summary": "Walks an application's accessibility tree and allocates a ref per element.",
            "confidential": false
        })
    );
}
