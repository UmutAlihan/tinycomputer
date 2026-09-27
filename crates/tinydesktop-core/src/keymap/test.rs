//! Tests for how each logical key is spelled on each platform and surface.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{Key, Platform};

const ALL: [Key; 15] = [
    Key::SelectAll,
    Key::Copy,
    Key::Cut,
    Key::Paste,
    Key::Undo,
    Key::Redo,
    Key::New,
    Key::Find,
    Key::Back,
    Key::Forward,
    Key::Refresh,
    Key::Settings,
    Key::Confirm,
    Key::Dismiss,
    Key::NextField,
];

#[test]
fn the_command_key_is_cmd_only_on_macos() {
    assert_eq!(Key::Paste.desktop(Platform::MacOs).unwrap(), "cmd+v");
    assert_eq!(Key::Paste.desktop(Platform::Windows).unwrap(), "ctrl+v");
    assert_eq!(Key::Paste.desktop(Platform::Linux).unwrap(), "ctrl+v");
    assert_eq!(Key::Paste.browser(Platform::MacOs).unwrap(), "Meta+v");
    assert_eq!(Key::Paste.browser(Platform::Linux).unwrap(), "Control+v");
}

#[test]
fn navigation_and_history_follow_platform_conventions() {
    assert_eq!(Key::Back.desktop(Platform::MacOs).unwrap(), "cmd+[");
    assert_eq!(Key::Back.desktop(Platform::Windows).unwrap(), "alt+left");
    assert_eq!(Key::Forward.desktop(Platform::MacOs).unwrap(), "cmd+]");
    assert_eq!(Key::Forward.desktop(Platform::Linux).unwrap(), "alt+right");
    assert_eq!(Key::Redo.desktop(Platform::MacOs).unwrap(), "cmd+shift+z");
    assert_eq!(Key::Redo.desktop(Platform::Windows).unwrap(), "ctrl+y");
    assert_eq!(Key::Settings.desktop(Platform::MacOs).unwrap(), "cmd+,");
    assert_eq!(Key::Settings.desktop(Platform::Windows), None);
}

#[test]
fn every_key_has_a_desktop_spelling_on_macos_and_plain_keys_match_everywhere() {
    for key in ALL {
        assert!(key.desktop(Platform::MacOs).is_some(), "{key:?}");
    }
    for platform in [Platform::MacOs, Platform::Windows, Platform::Linux] {
        assert_eq!(Key::Confirm.desktop(platform).unwrap(), "return");
        assert_eq!(Key::Dismiss.desktop(platform).unwrap(), "escape");
        assert_eq!(Key::NextField.desktop(platform).unwrap(), "tab");
        assert_eq!(Key::Confirm.browser(platform).unwrap(), "Enter");
    }
}

#[test]
fn a_web_page_has_no_new_or_settings_shortcut() {
    let browser = ALL
        .iter()
        .filter_map(|key| key.browser(Platform::Windows))
        .collect::<Vec<_>>();
    assert_eq!(browser.len(), 13);
    assert_eq!(Key::New.browser(Platform::MacOs), None);
    assert_eq!(Key::Settings.browser(Platform::MacOs), None);
    assert_eq!(Key::Back.browser(Platform::MacOs).unwrap(), "Alt+ArrowLeft");
    assert_eq!(Key::Refresh.browser(Platform::Linux).unwrap(), "F5");
}

#[test]
fn the_current_platform_matches_the_build_target() {
    let expected = if cfg!(target_os = "macos") {
        Platform::MacOs
    } else if cfg!(target_os = "windows") {
        Platform::Windows
    } else {
        Platform::Linux
    };
    assert_eq!(Platform::current(), expected);
}
