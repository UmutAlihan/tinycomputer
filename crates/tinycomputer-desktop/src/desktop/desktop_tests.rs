//! Unit tests for the engine's configuration, conversions, and envelope.
//!
//! These cover what can be asserted without a display server, a granted
//! permission, or a running application: the configuration parser, the
//! contract-to-engine conversions, the permission preflight, and the two
//! members that touch nothing outside this process. Anything that drives a real
//! application belongs in a live suite, not here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use agent_desktop_core::{PermissionReport, PermissionState};

mod config_tests;
mod convert_tests;
mod members_tests;
mod permission_tests;
mod reply_tests;

/// A report denying `accessibility`, `screen_recording`, or both.
fn denying(accessibility: bool, screen_recording: bool) -> PermissionReport {
    let denied = || PermissionState::Denied {
        suggestion: "Grant it in System Settings".to_owned(),
    };
    let mut report = PermissionReport::default();
    if accessibility {
        report.accessibility = denied();
    }
    if screen_recording {
        report.screen_recording = denied();
    }
    report
}
