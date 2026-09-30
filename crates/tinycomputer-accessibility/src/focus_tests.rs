//! Unit tests for the focus module's bounded command runner.

#![allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(unix)]
use super::*;

#[cfg(unix)]
#[test]
fn command_output_with_timeout_returns_output_for_fast_command() {
    let mut command = Command::new("sh");
    command.arg("-c").arg("printf ready");

    let output =
        command_output_with_timeout("test fast command", &mut command, Duration::from_secs(1))
            .expect("fast command should complete");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ready");
}

#[cfg(unix)]
#[test]
fn command_output_with_timeout_drains_large_output_while_waiting() {
    let mut command = Command::new("sh");
    command.arg("-c").arg("head -c 200000 /dev/zero");

    let output = command_output_with_timeout(
        "test large output command",
        &mut command,
        Duration::from_secs(2),
    )
    .expect("large output should not fill the pipe and block the child");

    assert!(output.status.success());
    assert_eq!(output.stdout.len(), 200_000);
}

#[cfg(unix)]
#[test]
fn command_output_with_timeout_kills_slow_command() {
    let mut command = Command::new("sh");
    command.arg("-c").arg("sleep 2; printf late");

    let error =
        command_output_with_timeout("test slow command", &mut command, Duration::from_millis(50))
            .expect_err("slow command should time out");

    assert!(error.contains("timed out after"));
}

#[cfg(not(target_os = "macos"))]
#[test]
fn public_focus_query_returns_typed_unsupported_error() {
    let error = super::focused_text_context().expect_err("focus querying is macOS-only");
    assert!(matches!(error, super::Error::UnsupportedPlatform));
}

fn sample_context(bounds: Option<super::super::types::ElementBounds>) -> FocusedTextContext {
    FocusedTextContext {
        app_name: Some("Editor".to_string()),
        role: Some("AXTextField".to_string()),
        text: String::new(),
        selected_text: None,
        raw_error: None,
        bounds,
    }
}

#[test]
fn validate_context_accepts_matching_element_bounds() {
    let bounds = super::super::types::ElementBounds {
        x: 1,
        y: 2,
        width: 3,
        height: 4,
    };
    assert!(
        validate_context(
            Some("editor"),
            Some("AXTextField"),
            Some(bounds),
            &sample_context(Some(bounds))
        )
        .is_ok()
    );
}

#[test]
fn validate_context_rejects_changed_or_unavailable_element_bounds() {
    use super::super::types::ElementBounds;
    let expected = ElementBounds {
        x: 1,
        y: 2,
        width: 3,
        height: 4,
    };
    let changed = ElementBounds {
        x: 9,
        y: 2,
        width: 3,
        height: 4,
    };
    assert!(matches!(
        validate_context(
            Some("Editor"),
            Some("AXTextField"),
            Some(expected),
            &sample_context(Some(changed))
        ),
        Err(Error::FocusTargetChanged)
    ));
    assert!(matches!(
        validate_context(
            Some("Editor"),
            Some("AXTextField"),
            Some(expected),
            &sample_context(None)
        ),
        Err(Error::FocusTargetChanged)
    ));
    assert!(matches!(
        validate_context(
            Some("Editor"),
            Some("AXTextField"),
            None,
            &sample_context(None)
        ),
        Err(Error::FocusTargetChanged)
    ));
}

#[test]
fn validate_context_reports_application_and_role_changes() {
    let context = sample_context(None);
    assert!(matches!(
        validate_context(Some("Other"), Some("AXTextField"), None, &context),
        Err(Error::FocusChanged { .. })
    ));
    assert!(matches!(
        validate_context(Some("Editor"), Some("AXButton"), None, &context),
        Err(Error::FocusRoleChanged { .. })
    ));
}
