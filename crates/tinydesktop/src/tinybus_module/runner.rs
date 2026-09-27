//! The flow runner behind the Agent members: each task's flow runs on a
//! [`Workspace`] joining the desktop and, once linked, the browser.

use tinydesktop_browser::BrowserSurface;
use tinydesktop_bus::DesktopResponse;
use tinydesktop_bus::agent::TaskConstraints;
use tinydesktop_engine::{FlowFuture, FlowRunner, JevRuntime, Workspace};

use crate::Desktop;

/// Runs task flows with the module's Jev runtime.
#[derive(Debug, Clone)]
pub(super) struct WorkspaceRunner {
    pub(super) desktop: Desktop,
    pub(super) jev: Option<JevRuntime>,
}

impl FlowRunner for WorkspaceRunner {
    fn run(
        &self,
        _constraints: &TaskConstraints,
        request: tinydesktop_bus::RunFlowRequest,
    ) -> FlowFuture {
        let Some(runtime) = self.jev.clone() else {
            return Box::pin(async { jev_not_configured("run-flow") });
        };
        // The browser engine is not linked into this build yet, so web steps
        // are refused with `BROWSER_NOT_AVAILABLE` rather than attempted.
        let workspace = Workspace::<Desktop, BrowserSurface>::new(self.desktop.clone(), None);
        Box::pin(tinydesktop_engine::run_flow(workspace, runtime, request))
    }
}

/// The reply an agentic member gives when no Jev runtime was configured.
pub(super) fn jev_not_configured(command: &str) -> DesktopResponse {
    DesktopResponse::err(
        command,
        tinydesktop_bus::DesktopError::new(
            "JEV_NOT_CONFIGURED",
            "Jev must be supplied through private module configuration",
        ),
    )
}
