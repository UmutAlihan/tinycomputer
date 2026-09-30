//! Tests for the public accessibility error categories and display messages.

use super::Error;

#[test]
fn displays_platform_and_operation_errors() {
    assert_eq!(
        Error::UnsupportedPlatform.to_string(),
        "operation is unsupported on this platform"
    );
    assert_eq!(
        Error::FocusQuery("helper timed out".to_string()).to_string(),
        "focused text query failed: helper timed out"
    );
    assert_eq!(
        Error::HelperTimeout("osascript exceeded 1500ms".to_string()).to_string(),
        "accessibility helper timed out: osascript exceeded 1500ms"
    );
    assert_eq!(
        Error::GlobeListener("helper failed".to_string()).to_string(),
        "Globe listener operation failed: helper failed"
    );
}

#[test]
fn focus_identity_errors_preserve_expected_and_actual_values() {
    let app = Error::FocusChanged {
        expected: "Editor".to_string(),
        actual: "Browser".to_string(),
    };
    assert_eq!(
        app.to_string(),
        "focus shifted from 'Editor' to 'Browser', aborting insertion"
    );

    let role = Error::FocusRoleChanged {
        expected: "AXTextArea".to_string(),
        actual: "AXButton".to_string(),
    };
    assert_eq!(
        role.to_string(),
        "focus role changed from 'AXTextArea' to 'AXButton', aborting insertion"
    );
}
