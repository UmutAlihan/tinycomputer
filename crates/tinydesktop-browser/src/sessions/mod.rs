//! [`Browser`]: sessions, the calls on them, and the outputs they produce.
//!
//! Each session owns one [`Engine`] and a private scratch directory where
//! agent-browser writes screenshots and downloads for this crate to collect.
//! Calls on one session are serialized; different sessions run independently.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tinydesktop_bus::browser::{
    Action, ActionOutcome, DownloadId, DownloadInfo, DownloadState, DownloadWaitRequest,
    EvaluateRequest, NavigateRequest, OutputChunk, OutputId, OutputRef, PageState, PageText,
    ReadFormat, ReadRequest, ScreenshotRequest, SessionId, SessionInfo, SessionOptions, Snapshot,
    SnapshotRequest, Target,
};

use crate::convert;
use crate::engine::{Engine, Launcher};
use crate::error::{Error, Result};
use crate::outputs::{OutputStore, within_cap};
use crate::reply;

/// How many sessions may be open at once.
pub const MAX_SESSIONS: usize = 8;

/// Browser automation over agent-browser, one engine per session.
#[derive(Debug)]
pub struct Browser {
    launcher: Arc<dyn Launcher>,
    sessions: Mutex<HashMap<SessionId, Arc<tokio::sync::Mutex<Session>>>>,
    outputs: Mutex<OutputStore>,
    scratch: PathBuf,
    counter: AtomicU64,
}

struct Session {
    engine: Box<dyn Engine>,
    info: SessionInfo,
    options: SessionOptions,
    sequence: u64,
    downloads: Vec<DownloadInfo>,
    dir: PathBuf,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Session")
            .field("info", &self.info)
            .field("sequence", &self.sequence)
            .finish_non_exhaustive()
    }
}

impl Session {
    async fn run(&mut self, command: Value) -> Result<Value> {
        reply::data(&self.engine.execute(command).await)
    }

    async fn page(&mut self) -> Result<PageState> {
        let url = reply::text(&self.run(json!({"action": "url"})).await?, "url");
        let title = reply::text(&self.run(json!({"action": "title"})).await?, "title");
        Ok(PageState {
            url,
            title,
            status: None,
        })
    }

    fn scratch_file(&self, stem: &str, sequence: u64, extension: &str) -> Result<String> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|error| Error::failed(format!("cannot create scratch space: {error}")))?;
        Ok(self
            .dir
            .join(format!("{stem}-{sequence}.{extension}"))
            .to_string_lossy()
            .into_owned())
    }
}

impl Browser {
    /// A browser whose sessions are opened by `launcher`.
    ///
    /// Screenshots and downloads are staged under the system temporary
    /// directory, one private folder per session, and removed as they are
    /// collected or the session closes.
    #[must_use]
    pub fn new(launcher: Arc<dyn Launcher>) -> Self {
        Self::with_scratch(
            launcher,
            std::env::temp_dir().join(format!("tinydesktop-browser-{}", std::process::id())),
        )
    }

    /// Like [`Browser::new`], staging files under `scratch`.
    #[must_use]
    pub fn with_scratch(launcher: Arc<dyn Launcher>, scratch: PathBuf) -> Self {
        Self {
            launcher,
            sessions: Mutex::new(HashMap::new()),
            outputs: Mutex::new(OutputStore::default()),
            scratch,
            counter: AtomicU64::new(0),
        }
    }

    /// Launches (or attaches to) a browser and returns the session owning it.
    ///
    /// # Errors
    ///
    /// [`Error::LimitExceeded`] when [`MAX_SESSIONS`] are open, and whatever
    /// the engine reports when the browser cannot be launched or reached.
    pub async fn open_session(&self, options: SessionOptions) -> Result<SessionInfo> {
        if self.lock_sessions()?.len() >= MAX_SESSIONS {
            return Err(Error::LimitExceeded {
                message: format!("at most {MAX_SESSIONS} browser sessions may be open"),
            });
        }
        let id = SessionId::new(format!("s-{}", self.next()));
        let mut session = Session {
            engine: self.launcher.open(id.as_str()),
            info: SessionInfo {
                id: id.clone(),
                endpoint: options.endpoint.clone().unwrap_or_default(),
                launched: options.endpoint.is_none(),
                headless: options.headless,
                viewport: options.viewport,
                url: String::new(),
                title: String::new(),
            },
            options,
            sequence: 0,
            downloads: Vec::new(),
            dir: self.scratch.join(id.as_str()),
        };
        session.run(convert::launch(&session.options)).await?;
        session.run(convert::viewport(&session.options)).await?;
        let page = session.page().await?;
        session.info.url = page.url;
        session.info.title = page.title;
        let info = session.info.clone();
        self.lock_sessions()?
            .insert(id, Arc::new(tokio::sync::Mutex::new(session)));
        Ok(info)
    }

    /// Closes a session and everything it owns. Closing one that is already
    /// gone succeeds.
    ///
    /// # Errors
    ///
    /// Only when the session table itself is unusable.
    pub async fn close_session(&self, id: &SessionId) -> Result<()> {
        let Some(session) = self.lock_sessions()?.remove(id) else {
            return Ok(());
        };
        let mut session = session.lock().await;
        // A browser that is already gone has nothing left to close.
        let _closed = session.run(json!({"action": "close"})).await;
        let _removed = std::fs::remove_dir_all(&session.dir);
        Ok(())
    }

    /// The sessions currently open, with the page each was last seen on.
    ///
    /// # Errors
    ///
    /// Only when the session table itself is unusable.
    pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let sessions = self.lock_sessions()?.values().cloned().collect::<Vec<_>>();
        let mut infos = Vec::with_capacity(sessions.len());
        for session in sessions {
            infos.push(session.lock().await.info.clone());
        }
        infos.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(infos)
    }

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
            tinydesktop_bus::browser::ImageFormat::Png => "png",
            tinydesktop_bus::browser::ImageFormat::Jpeg => "jpeg",
            tinydesktop_bus::browser::ImageFormat::Webp => "webp",
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

    /// Reads up to `len` bytes of a held output from `offset`.
    ///
    /// # Errors
    ///
    /// [`Error::NoSuchOutput`] when it is unknown or expired, and
    /// [`Error::InvalidInput`] when `offset` is past its end.
    pub fn read_output(&self, id: &OutputId, offset: u64, len: u64) -> Result<OutputChunk> {
        self.lock_outputs()?.read(id, offset, len)
    }

    /// Releases a held output early. Releasing one already gone succeeds.
    ///
    /// # Errors
    ///
    /// Only when the output store itself is unusable.
    pub fn release_output(&self, id: &OutputId) -> Result<()> {
        self.lock_outputs()?.release(id);
        Ok(())
    }

    /// Drops held outputs whose time to live has passed.
    ///
    /// Expiry also happens on every output call; a host runs this every
    /// [`SWEEP_INTERVAL`](crate::SWEEP_INTERVAL) so an abandoned screenshot
    /// is released even when no further call arrives.
    ///
    /// # Errors
    ///
    /// Only when the output store itself is unusable.
    pub fn sweep_outputs(&self) -> Result<()> {
        self.lock_outputs()?.expire();
        Ok(())
    }

    /// The downloads this session has completed through
    /// [`Browser::wait_download`].
    ///
    /// # Errors
    ///
    /// [`Error::NoSuchSession`].
    pub async fn list_downloads(&self, id: &SessionId) -> Result<Vec<DownloadInfo>> {
        let session = self.session(id)?;
        let session = session.lock().await;
        Ok(session.downloads.clone())
    }

    /// Waits for the next download to finish and reports where it landed.
    ///
    /// # Errors
    ///
    /// [`Error::NoSuchSession`], and [`Error::Timeout`] when nothing finishes
    /// in time.
    pub async fn wait_download(
        &self,
        id: &SessionId,
        request: DownloadWaitRequest,
    ) -> Result<DownloadInfo> {
        let session = self.session(id)?;
        let mut session = session.lock().await;
        let sequence = self.next();
        let target = session.scratch_file("download", sequence, "bin")?;
        let timeout = request
            .timeout_ms
            .unwrap_or(session.options.default_timeout_ms);
        let data = session
            .run(json!({"action": "waitfordownload", "path": target, "timeout": timeout}))
            .await?;
        let path = data
            .get("path")
            .and_then(Value::as_str)
            .map_or(target, str::to_owned);
        let size = std::fs::metadata(&path).map_or(0, |metadata| metadata.len());
        let info = DownloadInfo {
            sequence,
            id: DownloadId::new(format!("d-{sequence}")),
            url: String::new(),
            suggested_filename: std::path::Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            state: DownloadState::Completed,
            received_bytes: size,
            total_bytes: Some(size),
            path: Some(path),
        };
        session.downloads.push(info.clone());
        Ok(info)
    }

    fn session(&self, id: &SessionId) -> Result<Arc<tokio::sync::Mutex<Session>>> {
        self.lock_sessions()?
            .get(id)
            .cloned()
            .ok_or_else(|| Error::NoSuchSession { id: id.to_string() })
    }

    fn lock_sessions(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<SessionId, Arc<tokio::sync::Mutex<Session>>>>>
    {
        self.sessions
            .lock()
            .map_err(|_| Error::failed("the session table was poisoned by a panic"))
    }

    fn lock_outputs(&self) -> Result<std::sync::MutexGuard<'_, OutputStore>> {
        self.outputs
            .lock()
            .map_err(|_| Error::failed("the output store was poisoned by a panic"))
    }

    fn next(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::Relaxed) + 1
    }
}

#[cfg(test)]
mod test;
