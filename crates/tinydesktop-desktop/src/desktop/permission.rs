//! What each member needs granted, and the check that runs before it.
//!
//! Desktop automation fails in a particularly unhelpful way when a permission
//! is missing: an accessibility API called by an unauthorized process usually
//! returns an empty tree rather than an error, so a snapshot succeeds and finds
//! nothing, and a click reports that the element is not there. That is
//! indistinguishable from an application that genuinely has no such button.
//!
//! Checking first turns that into a `PERM_DENIED` naming the setting to change.
//! The needs below mirror the `agent-desktop` CLI's own policy table, so the
//! module and the CLI refuse the same commands for the same reasons.

use agent_desktop_core::{
    AdapterError, AppError, Deadline, DeliverySemantics, ErrorCode, PermissionReport,
    PlatformAdapter,
};

/// What a member needs granted before it can work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Need {
    /// Nothing. The member reads the module's own state, or asks the window
    /// server for a list every process may have.
    Nothing,
    /// Accessibility access: reading or driving another application's tree.
    Accessibility,
    /// Screen recording: capturing pixels.
    ScreenRecording,
    /// Both, for capturing a specific application or window — which means
    /// resolving it through the accessibility tree first.
    AccessibilityAndScreenRecording,
}

impl Need {
    /// Whether accessibility access is part of this need.
    fn accessibility(self) -> bool {
        matches!(
            self,
            Self::Accessibility | Self::AccessibilityAndScreenRecording
        )
    }

    /// Whether screen recording is part of this need.
    fn screen_recording(self) -> bool {
        matches!(
            self,
            Self::ScreenRecording | Self::AccessibilityAndScreenRecording
        )
    }
}

/// Fetches the platform's permission report when `need` calls for one.
///
/// A member needing nothing gets [`PermissionReport::default`] without a round
/// trip. The two members that report permissions themselves pass
/// [`Need::Accessibility`] so the report they return is the live one.
pub(super) fn report(
    need: Need,
    adapter: &dyn PlatformAdapter,
) -> Result<PermissionReport, AppError> {
    if need == Need::Nothing {
        return Ok(PermissionReport::default());
    }
    let deadline = Deadline::standard()?;
    adapter.permission_report(deadline).map_err(AppError::from)
}

/// Fails when `report` does not cover `need`.
pub(super) fn preflight(need: Need, report: &PermissionReport) -> Result<(), AppError> {
    if need.accessibility() && report.accessibility_denied() {
        return Err(denied(
            "Accessibility permission not granted",
            report
                .accessibility_suggestion()
                .unwrap_or("Grant Accessibility permission and retry"),
        ));
    }
    if need.screen_recording() && report.screen_recording_denied() {
        return Err(denied(
            "Screen Recording permission not granted",
            report
                .screen_recording_suggestion()
                .unwrap_or("Grant Screen Recording permission and retry"),
        ));
    }
    Ok(())
}

/// Builds the `PERM_DENIED` error, marked as never having reached the
/// application so a caller knows a retry after granting is safe.
fn denied(message: &str, suggestion: &str) -> AppError {
    AppError::Adapter(
        AdapterError::new(ErrorCode::PermDenied, message)
            .with_suggestion(suggestion)
            .with_disposition(DeliverySemantics::not_delivered()),
    )
}
