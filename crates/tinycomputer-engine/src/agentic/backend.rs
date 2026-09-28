//! The desktop as the goal and intent loops see it: an observation and an
//! operation, each run off the async runtime.

use serde_json::json;
use tinycomputer_bus::{
    DesktopResponse, JevOperation, RefRequest, ScrollRequest, SetValueRequest, WaitRequest,
};

use super::reply::internal_error;
use super::screen::{Candidate, Screen, observe};
use crate::Desktop;

pub(super) trait AgentBackend: Clone + Send + 'static {
    fn observe(
        &self,
        app: &str,
        window_id: Option<&str>,
        root: Option<&str>,
    ) -> Result<Screen, Box<DesktopResponse>>;
    fn execute(
        &self,
        operation: JevOperation,
        target: Option<Candidate>,
        text: Option<String>,
    ) -> DesktopResponse;
}

impl AgentBackend for Desktop {
    fn observe(
        &self,
        app: &str,
        window_id: Option<&str>,
        root: Option<&str>,
    ) -> Result<Screen, Box<DesktopResponse>> {
        observe(self, app, window_id, root)
    }

    fn execute(
        &self,
        operation: JevOperation,
        target: Option<Candidate>,
        text: Option<String>,
    ) -> DesktopResponse {
        execute_desktop(self, operation, target.as_ref(), text)
    }
}

pub(super) async fn observe_async<B: AgentBackend>(
    backend: B,
    app: String,
    window_id: Option<String>,
    root: Option<String>,
) -> Result<Screen, Box<DesktopResponse>> {
    tokio::task::spawn_blocking(move || {
        backend.observe(&app, window_id.as_deref(), root.as_deref())
    })
    .await
    .map_err(|error| {
        Box::new(internal_error(&format!(
            "desktop observation task failed: {error}"
        )))
    })?
}

pub(super) async fn execute_operation<B: AgentBackend>(
    backend: B,
    operation: JevOperation,
    target: Option<Candidate>,
    text: Option<String>,
) -> DesktopResponse {
    tokio::task::spawn_blocking(move || backend.execute(operation, target, text))
        .await
        .unwrap_or_else(|error| internal_error(&format!("desktop action task failed: {error}")))
}

pub(super) fn execute_desktop(
    desktop: &Desktop,
    operation: JevOperation,
    target: Option<&Candidate>,
    text: Option<String>,
) -> DesktopResponse {
    let ref_id = target.map(|node| node.ref_id.clone());
    match operation {
        JevOperation::Click => desktop.click(RefRequest::new(ref_id.unwrap_or_default())),
        JevOperation::TypeText => desktop.set_value(SetValueRequest {
            ref_id: ref_id.unwrap_or_default(),
            value: text.unwrap_or_default(),
            ..SetValueRequest::default()
        }),
        JevOperation::Check => desktop.check(RefRequest::new(ref_id.unwrap_or_default())),
        JevOperation::Uncheck => desktop.uncheck(RefRequest::new(ref_id.unwrap_or_default())),
        JevOperation::Expand => desktop.expand(RefRequest::new(ref_id.unwrap_or_default())),
        JevOperation::Collapse => desktop.collapse(RefRequest::new(ref_id.unwrap_or_default())),
        JevOperation::Scroll => desktop.scroll(ScrollRequest::new(
            ref_id.unwrap_or_default(),
            tinycomputer_bus::Direction::Down,
            3,
        )),
        JevOperation::Wait => desktop.wait(WaitRequest::sleep(500)),
        JevOperation::Drill | JevOperation::Widen => {
            DesktopResponse::ok("look", json!({"root": ref_id}))
        }
        JevOperation::Done | JevOperation::Blocked => {
            DesktopResponse::ok("resolve-intent", json!({}))
        }
    }
}
