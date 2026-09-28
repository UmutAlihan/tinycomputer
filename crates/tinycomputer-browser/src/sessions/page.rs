//! The calls on a session's page: navigation, snapshots, actions, reading,
//! evaluation, and screenshots.

use serde_json::Value;
use tinycomputer_bus::browser::{
    Action, ActionOutcome, EvaluateRequest, NavigateRequest, OutputRef, PageState, PageText,
    ReadFormat, ReadRequest, ScreenshotRequest, SessionId, Snapshot, SnapshotRequest, Target,
};

use super::Browser;
use crate::convert;
use crate::error::{Error, Result};
use crate::outputs::within_cap;
use crate::reply;

impl Browser {
    /// Navigates the session's active page.
    ///
    /// # Errors
    ///
    /// [`Error::NoSuchSession`], [`Error::InvalidInput`] for an empty URL,
    /// [`Error::BlockedByPolicy`] outside the allowed origins, and whatever
    /// else the navigation reports.
    pub async fn navigate(&self, id: &SessionId, request: NavigateRequest) -> Result<PageState> {
        let session = self.session(id)?;
        let mut session = session.lock().await;
        let data = session.run(convert::navigate(&request)?).await?;
        let page = PageState {
            url: reply::text(&data, "url"),
            title: reply::text(&data, "title"),
            status: None,
        };
        session.info.url.clone_from(&page.url);
        session.info.title.clone_from(&page.title);
        Ok(page)
    }

    /// Captures the page's accessibility tree with element refs.
    ///
    /// # Errors
    ///
    /// [`Error::NoSuchSession`], and whatever the snapshot reports.
    pub async fn snapshot(&self, id: &SessionId, request: SnapshotRequest) -> Result<Snapshot> {
        let session = self.session(id)?;
        let mut session = session.lock().await;
        let data = session.run(convert::snapshot(&request)).await?;
        let page = session.page().await?;
        session.sequence += 1;
        Ok(reply::snapshot(
            &data,
            page.url,
            page.title,
            session.sequence,
            request.max_chars,
        ))
    }

    /// Performs one interaction and reports the page it left.
    ///
    /// # Errors
    ///
    /// [`Error::NoSuchSession`], [`Error::InvalidInput`] for an action the
    /// engine cannot express, [`Error::StaleRef`] for a ref from an earlier
    /// snapshot, and whatever else the action reports.
    pub async fn perform(&self, id: &SessionId, action: Action) -> Result<ActionOutcome> {
        let session = self.session(id)?;
        let mut session = session.lock().await;
        let command = convert::action(&action, session.options.default_timeout_ms)?;
        let data = session.run(command).await?;
        let value = match &action {
            Action::GetText { .. } => data.get("text").cloned().unwrap_or(Value::Null),
            Action::GetAttribute { .. } => data.get("value").cloned().unwrap_or(Value::Null),
            Action::IsVisible { .. } => data.get("visible").cloned().unwrap_or(Value::Null),
            _ => Value::Null,
        };
        let matched = match &action {
            Action::Click {
                target: Target::Locator { value },
                ..
            }
            | Action::Fill {
                target: Target::Locator { value },
                ..
            } => Some(format!("{:?} {:?}", value.by, value.value)),
            _ => None,
        };
        let page = session.page().await?;
        session.info.url.clone_from(&page.url);
        session.info.title.clone_from(&page.title);
        Ok(ActionOutcome {
            value,
            page,
            matched,
        })
    }

    /// Extracts the active page as text, markdown, or HTML.
    ///
    /// # Errors
    ///
    /// [`Error::NoSuchSession`], and whatever the extraction reports.
    pub async fn read_page(&self, id: &SessionId, request: ReadRequest) -> Result<PageText> {
        let session = self.session(id)?;
        let mut session = session.lock().await;
        let data = session.run(convert::read(&request)).await?;
        let content = ["content", "text", "html"]
            .into_iter()
            .find_map(|key| data.get(key).and_then(Value::as_str))
            .unwrap_or_default();
        let truncated = content.chars().count() > request.max_chars
            || data.get("truncated").and_then(Value::as_bool) == Some(true);
        let content = content.chars().take(request.max_chars).collect();
        let page = session.page().await?;
        Ok(PageText {
            url: page.url,
            title: page.title,
            format: if request.format == ReadFormat::Markdown && request.selector.is_some() {
                ReadFormat::Text
            } else {
                request.format
            },
            content,
            truncated,
        })
    }

    /// Runs one raw agent-browser command, such as
    /// `{"action": "inputvalue", "selector": "@e3"}`, and returns its `data`.
    ///
    /// This is the escape hatch for engine capabilities the typed calls do
    /// not cover. It carries no policy of its own: a caller exposing it to a
    /// model must decide which actions to allow.
    ///
    /// # Errors
    ///
    /// [`Error::NoSuchSession`], [`Error::InvalidInput`] when `command` names
    /// no `action`, and whatever the engine reports.
    pub async fn command(&self, id: &SessionId, command: Value) -> Result<Value> {
        if command
            .get("action")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err(Error::invalid_input("a command needs an action"));
        }
        let session = self.session(id)?;
        let mut session = session.lock().await;
        session.run(command).await
    }

    /// Evaluates JavaScript in the page and returns its value.
    ///
    /// # Errors
    ///
    /// [`Error::NoSuchSession`], [`Error::InvalidInput`] for an empty
    /// expression, and [`Error::PageError`] when the script throws.
    pub async fn evaluate(&self, id: &SessionId, request: EvaluateRequest) -> Result<Value> {
        let session = self.session(id)?;
        let mut session = session.lock().await;
        let data = session.run(convert::evaluate(&request)?).await?;
        Ok(data.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Captures a screenshot and holds it for collection with
    /// [`Browser::read_output`].
    ///
    /// # Errors
    ///
    /// [`Error::NoSuchSession`], [`Error::InvalidInput`] for a bad quality or
    /// a locator target, [`Error::LimitExceeded`] for an oversized image, and
    /// whatever the capture reports.
    pub async fn screenshot(
        &self,
        id: &SessionId,
        request: ScreenshotRequest,
    ) -> Result<OutputRef> {
        let session = self.session(id)?;
        let mut session = session.lock().await;
        let extension = match request.format {
            tinycomputer_bus::browser::ImageFormat::Png => "png",
            tinycomputer_bus::browser::ImageFormat::Jpeg => "jpeg",
            tinycomputer_bus::browser::ImageFormat::Webp => "webp",
        };
        let path = session.scratch_file("shot", self.next(), extension)?;
        let data = session.run(convert::screenshot(&request, &path)?).await?;
        let written = data
            .get("path")
            .and_then(Value::as_str)
            .map_or_else(|| path.clone(), str::to_owned);
        let bytes = std::fs::read(&written)
            .map_err(|error| Error::failed(format!("screenshot was not written: {error}")))?;
        let _removed = std::fs::remove_file(&written);
        within_cap(bytes.len().div_ceil(3) * 4)?;
        let (width, height) = reply::image_size(&bytes);
        self.lock_outputs()?
            .insert(bytes, request.format.media_type(), width, height)
    }
}
