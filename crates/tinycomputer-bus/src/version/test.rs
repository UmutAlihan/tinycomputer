//! Unit tests for the contract version and its bind rule.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{CONTRACT_VERSION, binds, is_compatible};

#[test]
fn the_shipped_contract_version_is_pinned() {
    assert_eq!(CONTRACT_VERSION, (2, 4));
}

#[test]
fn the_contract_binds_to_itself() {
    assert!(is_compatible(CONTRACT_VERSION));
}

#[test]
fn a_newer_minor_on_the_module_side_binds() {
    assert!(is_compatible((2, 4)));
    assert!(is_compatible((2, 97)));
    // 2.4 added task rescues: a 2.4 host may set `budget.max_rescues`, a
    // 2.3 module would not understand it.
    assert!(!is_compatible((2, 3)));
    assert!(!is_compatible((2, 0)));
}

#[test]
fn an_older_minor_on_the_module_side_is_rejected() {
    // A host built against 1.8 cannot call a 1.7 module: the brief and vote
    // fields it sends are not understood there.
    assert!(!binds((1, 8), (1, 7)));
    assert!(binds((1, 8), (1, 8)));
    assert!(!binds((2, 1), (2, 0)));
}

#[test]
fn a_different_major_is_rejected() {
    assert!(!is_compatible((0, 0)));
    // 2.0 renamed the interface from `tinydesktop` to `tinycomputer`: a
    // 1.x host calls names a 2.x module does not serve.
    assert!(!is_compatible((1, 8)));
    assert!(!is_compatible((3, 0)));
}
