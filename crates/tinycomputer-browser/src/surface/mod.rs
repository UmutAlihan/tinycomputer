//! [`BrowserSurface`]: one browser session as a decision loop's
//! [`Surface`], so the flow runtime drives a web page the way it drives a
//! desktop application.
//!
//! Surface calls block, and the flow runtime makes them off its executor
//! (`spawn_blocking`); each one here runs the async [`Browser`] call to
//! completion on the runtime handle the surface was built with. The session
//! opens lazily, on the first call that needs a page.
//!
//! When the session has a window on screen, the agent's cursor glides onto
//! each element before the surface acts on it (`cursor.rs`). It is cosmetic:
//! the actions are the same with or without it.

mod card;
mod cursor;
mod envelope;
mod fields;
mod operations;
mod sight;
mod tree;

pub use sight::Denoised;

use std::sync::{Arc, Mutex};

use serde_json::json;
use tinycomputer_bus::DesktopResponse;
use tinycomputer_bus::browser::{Action, SessionId, SessionOptions};
use tinycomputer_core::Platform;
use tinycomputer_core::surface::Screen;
use tinycomputer_cursor::ScreenCursor;

use crate::error::{Error, Result};
use crate::sessions::Browser;
use envelope::reply;

/// How deep a skeleton observation reads before a flow drills in.
const SKELETON_DEPTH: u32 = 6;

/// How long the page is given to react once its network is quiet: long
/// enough for a banner or menu to finish closing.
const SETTLE_MS: u64 = 400;

/// The longest a settle waits for the page's network to go quiet; a page
/// that polls forever is never idle, so this is a cap, not an expectation.
const NETWORK_IDLE_MS: u64 = 2_000;

/// How a [`BrowserSurface`] reads a page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Perception {
    /// As a person looks at it: what is drawn and on top, the words on and
    /// beside each control, and which boxes take text, read from the
    /// rendered page. Falls back to the accessibility tree when it cannot
    /// reach what it sees (a shadow root, a frame in front) or the reading
    /// fails.
    #[default]
    Sight,
    /// Through the accessibility tree alone: roles and names as the page's
    /// markup declares them.
    Tree,
}

/// One browser session, lazily opened, as a [`Surface`].
#[derive(Clone)]
pub struct BrowserSurface {
    browser: Arc<Browser>,
    options: SessionOptions,
    session: Arc<Mutex<Option<SessionId>>>,
    handle: tokio::runtime::Handle,
    platform: Platform,
    cursor: Arc<ScreenCursor>,
    perception: Perception,
    denoised: Arc<Mutex<Denoised>>,
}

impl std::fmt::Debug for BrowserSurface {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BrowserSurface")
            .field("session", &self.session)
            .field("platform", &self.platform)
            .field("cursor", &self.cursor)
            .field("perception", &self.perception)
            .finish_non_exhaustive()
    }
}

impl BrowserSurface {
    /// A surface that opens its session on `browser` with `options`, and
    /// runs browser calls on `handle`. It draws no cursor until given one
    /// with [`BrowserSurface::with_cursor`].
    #[must_use]
    pub fn new(
        browser: Arc<Browser>,
        options: SessionOptions,
        handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            browser,
            options,
            session: Arc::new(Mutex::new(None)),
            handle,
            platform: Platform::current(),
            cursor: Arc::new(ScreenCursor::off()),
            perception: Perception::default(),
            denoised: Arc::new(Mutex::new(Denoised::default())),
        }
    }

    /// The same surface, reading pages with `perception`
    /// ([`Perception::Sight`] unless told otherwise).
    #[must_use]
    pub fn with_perception(mut self, perception: Perception) -> Self {
        self.perception = perception;
        self
    }

    /// The same surface, drawing on `cursor` — the screen's one agent
    /// cursor, shared with the desktop surface — whenever its session has a
    /// window on screen. The cursor is cosmetic: every action is performed
    /// the same way with or without it.
    #[must_use]
    pub fn with_cursor(mut self, cursor: Arc<ScreenCursor>) -> Self {
        self.cursor = cursor;
        self
    }

    /// The session this surface drives, once one is open.
    #[must_use]
    pub fn session(&self) -> Option<SessionId> {
        self.session.lock().ok().and_then(|session| session.clone())
    }

    /// What the last observation left out as noise: ads, blank boxes, and
    /// hidden content. Zero until a page is read, and when the last one was
    /// read through the accessibility tree rather than by sight.
    #[must_use]
    pub fn denoised(&self) -> Denoised {
        self.denoised
            .lock()
            .map(|denoised| *denoised)
            .unwrap_or_default()
    }

    /// Closes the session, if one is open, without waiting for it: safe to
    /// call from async code, where a blocking surface call is not.
    pub fn close(&self) {
        let Some(id) = self
            .session
            .lock()
            .ok()
            .and_then(|mut session| session.take())
        else {
            return;
        };
        let browser = self.browser.clone();
        self.handle.spawn(async move {
            let _closed = browser.close_session(&id).await;
        });
    }

    fn block<T>(&self, future: impl std::future::Future<Output = T>) -> T {
        self.handle.block_on(future)
    }

    fn ensure_session(&self) -> Result<SessionId> {
        let mut slot = self
            .session
            .lock()
            .map_err(|_| Error::failed("the browser surface was poisoned by a panic"))?;
        if let Some(id) = slot.as_ref() {
            return Ok(id.clone());
        }
        let info = self.block(self.browser.open_session(self.options.clone()))?;
        *slot = Some(info.id.clone());
        Ok(info.id)
    }

    /// Clicks the middle of `reference` even though something covers it,

    /// The page, or the part of it under `root`, read by sight; `None` when
    /// the reading fails or sees what it cannot reach, and the tree is read
    /// instead.
    fn see(&self, root: Option<&str>) -> Option<Screen> {
        self.keep_denoised(Denoised::default());
        let id = self.ensure_session().ok()?;
        let reply = self
            .block(self.browser.command(
                &id,
                json!({"action": "evaluate", "script": sight::script(root)}),
            ))
            .ok()?;
        let result = reply.get("result")?;
        let screen = sight::screen(result)?;
        self.keep_denoised(sight::denoised(result));
        Some(screen)
    }

    fn keep_denoised(&self, denoised: Denoised) {
        if let Ok(mut kept) = self.denoised.lock() {
            *kept = denoised;
        }
    }

    fn perform(&self, command: &str, action: Action) -> DesktopResponse {
        let outcome = self.ensure_session().and_then(|id| {
            self.block(self.browser.perform(&id, action))
                .map(|outcome| json!({"value": outcome.value, "url": outcome.page.url}))
        });
        reply(command, outcome)
    }
}

#[cfg(test)]
mod surface_tests;
