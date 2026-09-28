//! The `browser` configuration: how the module launches every browser it
//! opens, a task's or a `BrowserOpenSession` caller's.
//!
//! Booking sites turn away a browser that announces itself as headless, so a
//! host running tasks in a container sets a desktop `user_agent` and launch
//! `args` once here rather than on every request.

use tinycomputer_browser::{Perception, SessionOptions};

use crate::Result;

/// The browser settings the module applies to every session it opens.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BrowserDefaults {
    /// The Chrome or Chromium binary to launch, where the platform's own
    /// discovery would not find one.
    pub(crate) executable: Option<String>,
    /// The `User-Agent` every launched browser sends.
    pub(crate) user_agent: Option<String>,
    /// Extra command-line arguments for every launched browser.
    pub(crate) args: Vec<String>,
    /// How a task reads a page: by sight (the default) or the tree alone.
    pub(crate) perception: Perception,
}

impl BrowserDefaults {
    /// Reads the optional `browser` object: `executable` and `user_agent`
    /// strings, `args` an array of strings, and `perception` either `sight`
    /// or `tree`.
    ///
    /// # Errors
    ///
    /// [`crate::Error::ConfigFieldType`] when `browser` or any of its fields
    /// has the wrong shape; an unknown field is refused too, so a misspelt
    /// setting cannot be silently ignored.
    pub(crate) fn from_config(config: &serde_json::Value) -> Result<Self> {
        let Some(browser) = config.as_object().and_then(|object| object.get("browser")) else {
            return Ok(Self::default());
        };
        let invalid = || crate::Error::ConfigFieldType {
            field: "browser",
            expected: "an object with optional `executable` and `user_agent` strings, an `args` \
                       array of strings, and a `perception` of sight or tree",
        };
        let browser = browser.as_object().ok_or_else(invalid)?;
        let text = |name: &str| match browser.get(name) {
            None => Ok(None),
            Some(serde_json::Value::String(value)) => Ok(Some(value.clone())),
            Some(_) => Err(invalid()),
        };
        let mut defaults = Self {
            executable: text("executable")?,
            user_agent: text("user_agent")?,
            ..Self::default()
        };
        if let Some(args) = browser.get("args") {
            defaults.args = args
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .map(|arg| arg.as_str().map(str::to_owned).ok_or_else(invalid))
                .collect::<Result<_>>()?;
        }
        defaults.perception = match text("perception")?.as_deref() {
            None | Some("sight") => Perception::Sight,
            Some("tree") => Perception::Tree,
            Some(_) => return Err(invalid()),
        };
        if browser
            .keys()
            .any(|key| !["executable", "user_agent", "args", "perception"].contains(&key.as_str()))
        {
            return Err(invalid());
        }
        Ok(defaults)
    }

    /// `options` with these defaults filled in where the caller left them
    /// unset. An attached session launches nothing, so it takes no
    /// executable and no launch arguments.
    pub(crate) fn apply(&self, mut options: SessionOptions) -> SessionOptions {
        if options.endpoint.is_none() {
            if options.executable.is_none() {
                options.executable.clone_from(&self.executable);
            }
            if options.args.is_empty() {
                options.args.clone_from(&self.args);
            }
        }
        if options.user_agent.is_none() {
            options.user_agent.clone_from(&self.user_agent);
        }
        options
    }
}
