//! Tests for the `live_probe` binary: a reply without a tree, and the probe's\ndepth limit.

use super::{print_matching, require_tree};
use serde_json::json;
use tinycomputer_bus::DesktopResponse;

#[test]
fn successful_reply_without_tree_is_an_error() {
    let reply = DesktopResponse::ok("snapshot", json!({"app": "Example"}));
    assert!(require_tree(&reply).is_err());
    let reply = DesktopResponse::ok("snapshot", json!({"tree": null}));
    assert!(require_tree(&reply).is_err());
}

#[test]
fn deep_tree_reports_incomplete_probe() {
    let mut tree = json!({"role": "button", "name": "target"});
    for _ in 0..33 {
        tree = json!({"role": "group", "children": [tree]});
    }
    assert!(print_matching(&tree, "target", "", 0).is_err());
}

#[test]
fn boundary_depth_is_included() {
    let mut tree = json!({"role": "button", "name": "target"});
    for _ in 0..32 {
        tree = json!({"role": "group", "children": [tree]});
    }
    assert!(matches!(print_matching(&tree, "target", "", 0), Ok(1)));
}
