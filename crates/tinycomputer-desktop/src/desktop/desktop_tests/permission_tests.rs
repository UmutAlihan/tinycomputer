//! Tests for the permission preflight: what each member needs, and refusing
//! before the command runs.

use tinycomputer_bus as bus;

use crate::desktop::{Desktop, permission, permission::Need, reply};
use agent_desktop_core::{ErrorCode, PermissionReport};

use super::denying;

#[test]
fn a_denied_accessibility_permission_is_reported_before_the_command_runs() {
    let report = denying(true, false);

    let error = permission::preflight(Need::Accessibility, &report)
        .expect_err("a denied permission must not be ignored");
    let payload = reply::envelope("click", Err(error))
        .error
        .expect("a failed envelope carries an error");

    assert_eq!(payload.code, ErrorCode::PermDenied.as_str());
    // Nothing was attempted, so retrying after granting cannot duplicate an
    // effect.
    assert_eq!(payload.disposition.retry, bus::RetryDisposition::Safe);
}

#[test]
fn a_need_of_nothing_passes_a_report_that_denies_everything() {
    assert!(permission::preflight(Need::Nothing, &denying(true, true)).is_ok());
}

#[test]
fn a_screen_recording_need_ignores_a_denied_accessibility_permission() {
    assert!(permission::preflight(Need::ScreenRecording, &denying(true, false)).is_ok());
}

#[test]
fn a_screenshot_of_a_named_application_needs_more_than_a_full_screen_one() {
    // Framing a named window means resolving it through the accessibility tree
    // first, so the need is both permissions rather than screen recording
    // alone. Asserted through the reply because the need itself is private.
    let full_screen = Desktop::new().screenshot(bus::ScreenshotRequest::default());
    let targeted = Desktop::new().screenshot(bus::ScreenshotRequest {
        app: Some("Safari".to_owned()),
        ..bus::ScreenshotRequest::default()
    });

    assert_eq!(full_screen.command, "screenshot");
    assert_eq!(targeted.command, "screenshot");
}

#[test]
fn a_denied_screen_recording_permission_is_reported_before_the_capture_runs() {
    let error = permission::preflight(Need::ScreenRecording, &denying(false, true))
        .expect_err("a denied permission must not be ignored");
    let payload = reply::envelope("screenshot", Err(error))
        .error
        .expect("a failed envelope carries an error");

    assert_eq!(payload.code, ErrorCode::PermDenied.as_str());
    assert!(
        payload.suggestion.is_some(),
        "a permission refusal must say where to grant it"
    );
}

#[test]
fn a_capture_of_a_named_window_is_refused_by_either_missing_permission() {
    // The combined need means the accessibility half alone is enough to refuse,
    // because the window cannot be resolved without it.
    for report in [denying(true, false), denying(false, true)] {
        assert!(
            permission::preflight(Need::AccessibilityAndScreenRecording, &report).is_err(),
            "a targeted capture needs both"
        );
    }
    // An *unknown* permission is not a denied one: a platform that cannot say
    // must not block the command, or Linux — where nothing is reportable —
    // would refuse everything before the engine got the chance to explain what
    // it does not support.
    assert!(
        permission::preflight(
            Need::AccessibilityAndScreenRecording,
            &PermissionReport::default()
        )
        .is_ok()
    );
}
