//! [`Browser`]: sessions, the calls on them, and the outputs they produce.
//!
//! Each session owns one [`Engine`] and a private scratch directory where
//! agent-browser writes screenshots and downloads for this crate to collect.
//! Calls on one session are serialized; different sessions run independently.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tinycomputer_bus::browser::{DownloadInfo, PageState, SessionId, SessionInfo, SessionOptions};

use crate::convert;
use crate::engine::{Engine, Launcher};
use crate::error::{Error, Result};
use crate::outputs::OutputStore;
use crate::reply;

mod artifacts;
mod page;

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
    /// Sessions being launched: they count against [`MAX_SESSIONS`] from
    /// the moment the limit is checked, so two concurrent opens cannot both
    /// see room for the last slot.
    opening: AtomicUsize,
}

/// One reserved launch slot, given back when the launch ends — inserted,
/// failed, or dropped mid-launch by a cancelled caller.
struct Reservation<'a>(&'a AtomicUsize);

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
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
            std::env::temp_dir().join(format!("tinycomputer-browser-{}", std::process::id())),
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
            opening: AtomicUsize::new(0),
        }
    }

    /// Launches (or attaches to) a browser and returns the session owning it.
    ///
    /// # Errors
    ///
    /// [`Error::LimitExceeded`] when [`MAX_SESSIONS`] are open, and whatever
    /// the engine reports when the browser cannot be launched or reached.
    pub async fn open_session(&self, options: SessionOptions) -> Result<SessionInfo> {
        // Checked and reserved under the table's lock, so the check and the
        // claim are one step for every concurrent caller.
        let _reservation = {
            let sessions = self.lock_sessions()?;
            if sessions.len() + self.opening.load(Ordering::SeqCst) >= MAX_SESSIONS {
                return Err(Error::LimitExceeded {
                    message: format!("at most {MAX_SESSIONS} browser sessions may be open"),
                });
            }
            self.opening.fetch_add(1, Ordering::SeqCst);
            Reservation(&self.opening)
        };
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
mod sessions_tests;
