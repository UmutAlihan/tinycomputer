//! The members that report on the module and the machine it runs on.

use agent_desktop_core::commands::{permissions, status, version};
use tinydesktop_bus::{DesktopResponse, PermissionsRequest};

use super::{Desktop, Need};

impl Desktop {
    /// Reports the engine version, architecture, and operating system.
    ///
    /// Needs nothing granted and touches no other application, which makes it
    /// the cheapest way to confirm the module loaded and answers.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinydesktop_desktop::Desktop;
    /// let reply = Desktop::new().version();
    ///
    /// assert!(reply.ok);
    /// assert!(reply.data.and_then(|data| data.get("version").cloned()).is_some());
    /// ```
    #[must_use]
    pub fn version(&self) -> DesktopResponse {
        self.run("version", Need::Nothing, |_adapter, _context| {
            version::execute()
        })
    }

    /// Reports permissions, the active session, and the latest snapshot.
    ///
    /// The one call that answers "why did that not work" without guessing: it
    /// says whether the permission is granted and whether any refs exist to
    /// spend.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinydesktop_desktop::Desktop;
    /// let reply = Desktop::new().status();
    /// assert_eq!(reply.command, "status");
    /// ```
    #[must_use]
    pub fn status(&self) -> DesktopResponse {
        // The report is fetched but not preflighted: reporting a denied
        // permission is exactly what this member is for, so refusing to run
        // because one is denied would be circular.
        self.run_with_report("status", Need::Nothing, |adapter, context, _report| {
            status::execute_with_report_with_context(adapter, &live_report(adapter), context)
        })
    }

    /// Reports, and optionally prompts for, the permissions automation needs.
    ///
    /// Prompting puts a system dialog in front of whoever is at the machine, so
    /// it happens only when [`tinydesktop_bus::PermissionsRequest::request`] is
    /// set.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinydesktop_desktop::{Desktop, PermissionsRequest};
    /// let reply = Desktop::new().permissions(PermissionsRequest::default());
    /// assert_eq!(reply.command, "permissions");
    /// ```
    #[must_use]
    pub fn permissions(&self, request: PermissionsRequest) -> DesktopResponse {
        self.run("permissions", Need::Nothing, |adapter, _context| {
            permissions::execute_with_report(
                permissions::PermissionsArgs {
                    request: request.request,
                },
                adapter,
                &live_report(adapter),
            )
        })
    }
}

/// Reads the platform's permission report, falling back to the default when it
/// cannot be read.
///
/// The two members here exist to *report* on permissions, so a report that
/// cannot be fetched must not turn into a failed call: the default report says
/// "not granted", which is the honest answer when the platform will not say.
fn live_report(
    adapter: &dyn agent_desktop_core::PlatformAdapter,
) -> agent_desktop_core::PermissionReport {
    agent_desktop_core::Deadline::standard()
        .ok()
        .and_then(|deadline| adapter.permission_report(deadline).ok())
        .unwrap_or_default()
}
