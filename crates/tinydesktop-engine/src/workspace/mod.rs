//! [`Workspace`]: the desktop and the browser as one surface, so a single
//! flow can move between an application and a web page.
//!
//! Calls that name an application route by that name: `browser` (or an
//! address) goes to the browser, anything else to the desktop. Calls that do
//! not name one — acting on a candidate, reading it back — go to whichever
//! side was observed or opened last, which is the side the candidate came
//! from.

use std::sync::{Arc, Mutex};

use tinydesktop_bus::{DesktopError, DesktopResponse, JevOperation};
use tinydesktop_core::surface::{Candidate, Depth, Screen, Surface};

/// The application name that routes to the browser.
pub const BROWSER: &str = "browser";

/// Which side of a [`Workspace`] a call goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Desktop,
    Browser,
}

/// A desktop surface and, when one is configured, a browser surface.
#[derive(Debug, Clone)]
pub struct Workspace<D, W> {
    desktop: D,
    browser: Option<W>,
    active: Arc<Mutex<Side>>,
}

impl<D: Surface, W: Surface> Workspace<D, W> {
    /// A workspace over `desktop`, with `browser` when the browser is
    /// available to this task.
    #[must_use]
    pub fn new(desktop: D, browser: Option<W>) -> Self {
        Self {
            desktop,
            browser,
            active: Arc::new(Mutex::new(Side::Desktop)),
        }
    }

    /// Whether this workspace can reach a browser.
    #[must_use]
    pub fn has_browser(&self) -> bool {
        self.browser.is_some()
    }

    fn side_for(app: &str) -> Side {
        let app = app.trim().to_ascii_lowercase();
        if app == BROWSER
            || app.starts_with("browser:")
            || app.starts_with("http://")
            || app.starts_with("https://")
        {
            Side::Browser
        } else {
            Side::Desktop
        }
    }

    fn activate(&self, side: Side) {
        if let Ok(mut active) = self.active.lock() {
            *active = side;
        }
    }

    fn active(&self) -> Side {
        self.active.lock().map_or(Side::Desktop, |active| *active)
    }

    /// Runs `call` on the side named by `side`, or refuses when that side is
    /// the browser and there is none.
    fn on(
        &self,
        side: Side,
        command: &str,
        desktop: impl FnOnce(&D) -> DesktopResponse,
        browser: impl FnOnce(&W) -> DesktopResponse,
    ) -> DesktopResponse {
        match (side, &self.browser) {
            (Side::Desktop, _) => desktop(&self.desktop),
            (Side::Browser, Some(surface)) => browser(surface),
            (Side::Browser, None) => no_browser(command),
        }
    }
}

fn no_browser(command: &str) -> DesktopResponse {
    DesktopResponse::err(
        command,
        DesktopError::new(
            "BROWSER_NOT_AVAILABLE",
            "this task has no browser; allow the browser surface to use web pages",
        ),
    )
}

impl<D: Surface + Sync, W: Surface + Sync> Surface for Workspace<D, W> {
    fn observe(
        &self,
        app: &str,
        root: Option<&str>,
        depth: Depth,
    ) -> Result<Screen, Box<DesktopResponse>> {
        let side = Self::side_for(app);
        let screen = match (side, &self.browser) {
            (Side::Desktop, _) => self.desktop.observe(app, root, depth),
            (Side::Browser, Some(browser)) => browser.observe(app, root, depth),
            (Side::Browser, None) => Err(Box::new(no_browser("snapshot"))),
        }?;
        self.activate(side);
        Ok(screen)
    }

    fn execute(
        &self,
        operation: JevOperation,
        target: Option<Candidate>,
        text: Option<String>,
    ) -> DesktopResponse {
        self.on(
            self.active(),
            "execute",
            |desktop| desktop.execute(operation, target.clone(), text.clone()),
            |browser| browser.execute(operation, target.clone(), text.clone()),
        )
    }

    fn read_value(&self, target: &Candidate) -> Option<String> {
        match (self.active(), &self.browser) {
            (Side::Browser, Some(browser)) => browser.read_value(target),
            (Side::Browser, None) => None,
            (Side::Desktop, _) => self.desktop.read_value(target),
        }
    }

    fn paste(&self, app: &str, target: &Candidate, text: &str) -> DesktopResponse {
        self.on(
            Self::side_for(app),
            "paste",
            |desktop| desktop.paste(app, target, text),
            |browser| browser.paste(app, target, text),
        )
    }

    fn press(&self, app: &str, combo: &str) -> DesktopResponse {
        self.on(
            Self::side_for(app),
            "press",
            |desktop| desktop.press(app, combo),
            |browser| browser.press(app, combo),
        )
    }

    fn launch(&self, app: &str) -> DesktopResponse {
        let side = Self::side_for(app);
        let reply = self.on(
            side,
            "launch",
            |desktop| desktop.launch(app),
            |browser| browser.launch(app),
        );
        if reply.ok {
            self.activate(side);
        }
        reply
    }

    fn settle(&self) {
        match (self.active(), &self.browser) {
            (Side::Browser, Some(browser)) => browser.settle(),
            (Side::Browser, None) => {}
            (Side::Desktop, _) => self.desktop.settle(),
        }
    }

    fn navigate(&self, url: &str) -> DesktopResponse {
        let reply = self.on(
            Side::Browser,
            "navigate",
            |_| no_browser("navigate"),
            |browser| browser.navigate(url),
        );
        if reply.ok {
            self.activate(Side::Browser);
        }
        reply
    }
}

#[cfg(test)]
mod test;
