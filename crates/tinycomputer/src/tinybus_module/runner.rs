//! The flow runner behind the Agent members: each task's flow runs on a
//! [`Workspace`] joining the desktop and a browser session of its own.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tinycomputer_browser::{Browser, BrowserSurface, ScreenCursor, SessionOptions};
use tinycomputer_bus::DesktopResponse;
use tinycomputer_bus::agent::{SurfaceKind, TaskConstraints, TaskId};
use tinycomputer_engine::{FlowFuture, FlowRunner, JevRuntime, TextFuture, Workspace};

use super::browser_defaults::BrowserDefaults;
use crate::Desktop;

type TaskWorkspace = Workspace<Desktop, BrowserSurface>;

/// Runs task flows with the module's Jev runtime, on one workspace per task
/// so a continuation picks up where the last run left off.
#[derive(Debug)]
pub(super) struct WorkspaceRunner {
    pub(super) desktop: Desktop,
    pub(super) jev: Option<JevRuntime>,
    pub(super) browser: Arc<Browser>,
    /// How every task's browser launches, and how it reads a page.
    pub(super) defaults: BrowserDefaults,
    /// The screen's one agent cursor, which each task's browser shares with
    /// the desktop.
    pub(super) cursor: Arc<ScreenCursor>,
    pub(super) workspaces: Mutex<HashMap<TaskId, (TaskWorkspace, Option<BrowserSurface>)>>,
}

impl WorkspaceRunner {
    /// A runner with no task workspaces yet, opening each task's browser
    /// session on `browser` — the one the browser members serve, so a
    /// caller can inspect a task's session and read what it captured.
    pub(super) fn new(desktop: Desktop, jev: Option<JevRuntime>, browser: Arc<Browser>) -> Self {
        Self {
            desktop,
            jev,
            browser,
            defaults: BrowserDefaults::default(),
            cursor: Arc::new(ScreenCursor::off()),
            workspaces: Mutex::new(HashMap::new()),
        }
    }

    /// The task's workspace, created on first use: the desktop, and a
    /// browser session shaped by `constraints` unless they exclude the
    /// browser. Needs a Tokio runtime, which every caller runs on.
    fn workspace(&self, task: &TaskId, constraints: &TaskConstraints) -> TaskWorkspace {
        let fresh = || {
            let desktop = (constraints.surfaces.is_empty()
                || constraints.surfaces.contains(&SurfaceKind::Desktop))
            .then(|| self.desktop.clone());
            let browser = (constraints.surfaces.is_empty()
                || constraints.surfaces.contains(&SurfaceKind::Browser))
            .then(|| {
                BrowserSurface::new(
                    self.browser.clone(),
                    self.defaults.apply(SessionOptions {
                        endpoint: constraints.browser_endpoint.clone(),
                        headless: !constraints.headed,
                        allowed_origins: constraints.origins.clone(),
                        ..SessionOptions::default()
                    }),
                    tokio::runtime::Handle::current(),
                )
                .with_cursor(self.cursor.clone())
                .with_perception(self.defaults.perception)
            });
            (Workspace::new(desktop, browser.clone()), browser)
        };
        self.workspaces.lock().map_or_else(
            |_| fresh().0,
            |mut workspaces| {
                workspaces
                    .entry(task.clone())
                    .or_insert_with(fresh)
                    .0
                    .clone()
            },
        )
    }
}

impl FlowRunner for WorkspaceRunner {
    fn run(
        &self,
        task: &TaskId,
        constraints: &TaskConstraints,
        request: tinycomputer_bus::RunFlowRequest,
    ) -> FlowFuture {
        let Some(runtime) = self.jev.as_ref() else {
            return Box::pin(async { jev_not_configured("run-flow") });
        };
        // Every run of one task — the first flow and each continuation —
        // journals to one file, so a task reads back as one story.
        let runtime = runtime.journaled_as(&format!("task-{task}"));
        Box::pin(tinycomputer_engine::run_flow(
            self.workspace(task, constraints),
            runtime,
            request,
        ))
    }

    fn visible_text(&self, task: &TaskId) -> TextFuture {
        let workspace = self.workspace(task, &TaskConstraints::default());
        Box::pin(async move {
            tokio::task::spawn_blocking(move || workspace.visible_text())
                .await
                .unwrap_or_default()
        })
    }

    fn release(&self, task: &TaskId) {
        let released = self
            .workspaces
            .lock()
            .ok()
            .and_then(|mut workspaces| workspaces.remove(task));
        if let Some((_, Some(browser))) = released {
            browser.close();
        }
    }
}

/// The reply an agentic member gives when no Jev runtime was configured.
pub(super) fn jev_not_configured(command: &str) -> DesktopResponse {
    DesktopResponse::err(
        command,
        tinycomputer_bus::DesktopError::new(
            "JEV_NOT_CONFIGURED",
            "Jev must be supplied through private module configuration",
        ),
    )
}
