//! Tests that the declared manifest, the dispatch table, and the contract agree.

use super::service;
use crate::tinybus_module::{DesktopService, setup};
use serde_json::json;
use tinybus::broker::Broker;
use tinybus::transport::memory::MemoryBus;
use tinybus::{Connection, Interface};
use tinycomputer_bus::{DesktopResponse, PermissionsRequest, names};

/// The `methods = [...]` list `module_export!` was handed, read back out of
/// this module's own source.
///
/// The macro turns that list into an `extern "C"` function returning a raw
/// slice, and reading one back needs `unsafe`, which this workspace forbids.
/// Reading the literals the macro was given is the same assertion — that the
/// declared manifest and the contract agree — reached the safe way.
fn manifest_methods() -> Vec<String> {
    let source = include_str!("mod.rs");
    let (_, rest) = source
        .split_once("    methods = [")
        .expect("the module declares a methods list");
    let (list, _) = rest
        .split_once("\n    ]")
        .expect("the methods list is closed on its own line");

    list.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

#[test]
fn declared_methods_match_the_dispatch_table() {
    let methods = service()
        .members()
        .into_iter()
        .map(|member| member.to_string())
        .collect::<Vec<_>>();

    assert_eq!(methods, names::METHODS.to_vec());
}

#[test]
fn the_embedded_manifest_matches_the_contract() {
    assert_eq!(manifest_methods(), names::METHODS.to_vec());
}

#[test]
fn the_served_interface_name_matches_the_contract() {
    assert_eq!(service().name().to_string(), names::INTERFACE);
}

#[test]
fn the_type_member_keeps_the_engines_spelling_rather_than_the_rust_one() {
    // The Rust method is `type_text` because `type` is a keyword; the wire name
    // has to stay `Type` to match the contract a host spells.
    let members = service()
        .members()
        .into_iter()
        .map(|member| member.to_string())
        .collect::<Vec<_>>();

    assert!(members.contains(&"Type".to_owned()));
    assert!(!members.contains(&"TypeText".to_owned()));
}
