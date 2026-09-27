//! The flow runner behind the Agent members: each task's flow runs on a
//! [`Workspace`] joining the desktop and, once linked, the browser.

use std::collections::HashMap;
use std::sync::Mutex;

use tinydesktop_browser::BrowserSurface;
use tinydesktop_bus::DesktopResponse;
use tinydesktop_bus::agent::{TaskConstraints, TaskId};
use tinydesktop_engine::{FlowFuture, FlowRunner, JevRuntime, TextFuture, Workspace};

use crate::Desktop;

type TaskWorkspace = Workspace<Desktop, BrowserSurface>;

/// Runs task flows with the module's Jev runtime, on one workspace per task
/// so a continuation picks up where the last run left off.
#[derive(Debug)]
pub(super) struct WorkspaceRunner {
    pub(super) desktop: Desktop,
    pub(super) jev: Option<JevRuntime>,
    pub(super) workspaces: Mutex<HashMap<TaskId, TaskWorkspace>>,
}

impl WorkspaceRunner {
    /// A runner with no task workspaces yet.
    pub(super) fn new(desktop: Desktop, jev: Option<JevRuntime>) -> Self {
        Self {
            desktop,
            jev,
            workspaces: Mutex::new(HashMap::new()),
        }
    }

    fn workspace(&self, task: &TaskId) -> TaskWorkspace {
        let fresh = || {
            // The browser engine is not linked into this build yet, so web
            // steps are refused with `BROWSER_NOT_AVAILABLE`.
            Workspace::new(self.desktop.clone(), None)
        };
        self.workspaces.lock().map_or_else(
            |_| fresh(),
            |mut workspaces| workspaces.entry(task.clone()).or_insert_with(fresh).clone(),
        )
    }
}

impl FlowRunner for WorkspaceRunner {
    fn run(
        &self,
        task: &TaskId,
        _constraints: &TaskConstraints,
        request: tinydesktop_bus::RunFlowRequest,
    ) -> FlowFuture {
        let Some(runtime) = self.jev.clone() else {
            return Box::pin(async { jev_not_configured("run-flow") });
        };
        Box::pin(tinydesktop_engine::run_flow(
            self.workspace(task),
            runtime,
            request,
        ))
    }

    fn visible_text(&self, task: &TaskId) -> TextFuture {
        let workspace = self.workspace(task);
        Box::pin(async move {
            tokio::task::spawn_blocking(move || workspace.visible_text())
                .await
                .unwrap_or_default()
        })
    }

    fn release(&self, task: &TaskId) {
        if let Ok(mut workspaces) = self.workspaces.lock() {
            workspaces.remove(task);
        }
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
