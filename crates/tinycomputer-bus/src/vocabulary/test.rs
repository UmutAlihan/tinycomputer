//! Unit tests pinning the shared enumerations' wire forms.
//!
//! Each of these mirrors an `agent-desktop-core` type. The spellings below are
//! the engine's own, so a divergence shows up here rather than as an
//! `INVALID_ARGS` reply a host cannot explain.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    ClipboardFormat, Direction, ElementProperty, ElementStateProperty, Modifier, MouseButton,
    StatePredicate, Surface,
};
use serde_json::json;

fn encode<T: serde::Serialize>(value: T) -> serde_json::Value {
    serde_json::to_value(value).expect("the value serializes")
}

#[test]
fn surfaces_are_snake_case_on_the_wire() {
    assert_eq!(encode(Surface::Window), json!("window"));
    assert_eq!(encode(Surface::SystemTray), json!("system_tray"));
    assert_eq!(encode(Surface::MenuBarExtras), json!("menu_bar_extras"));
    assert_eq!(
        encode(Surface::NotificationCenter),
        json!("notification_center")
    );
}

#[test]
fn the_default_surface_is_the_window() {
    assert_eq!(Surface::default(), Surface::Window);
}

#[test]
fn directions_and_buttons_are_pascal_case_on_the_wire() {
    // The engine derives these without a rename attribute, so the variant name
    // is the wire name.
    assert_eq!(encode(Direction::Down), json!("Down"));
    assert_eq!(encode(MouseButton::Middle), json!("Middle"));
    assert_eq!(MouseButton::default(), MouseButton::Left);
}

#[test]
fn the_macos_spelling_of_the_meta_modifier_is_accepted() {
    let meta: Modifier = serde_json::from_value(json!("Cmd")).expect("Cmd decodes");
    assert_eq!(meta, Modifier::Meta);
    // It encodes back to the canonical name, not the alias.
    assert_eq!(encode(Modifier::Meta), json!("Meta"));
}

#[test]
fn clipboard_formats_are_snake_case_and_default_to_text() {
    assert_eq!(encode(ClipboardFormat::FileUrls), json!("file_urls"));
    assert_eq!(ClipboardFormat::default(), ClipboardFormat::Text);
}

#[test]
fn element_properties_are_snake_case_on_the_wire() {
    assert_eq!(encode(ElementProperty::Bounds), json!("bounds"));
    assert_eq!(encode(ElementStateProperty::Expanded), json!("expanded"));
}

#[test]
fn a_bare_state_predicate_leaves_the_expectation_absent() {
    assert_eq!(
        encode(StatePredicate::set("enabled")),
        json!({ "token": "enabled" })
    );
    assert_eq!(
        encode(StatePredicate::expect("checked", false)),
        json!({ "token": "checked", "expected": false })
    );
}

#[test]
fn a_state_predicate_decodes_without_an_expectation() {
    let predicate: StatePredicate =
        serde_json::from_value(json!({ "token": "focused" })).expect("it decodes");
    assert_eq!(predicate, StatePredicate::set("focused"));
}
