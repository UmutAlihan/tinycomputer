//! The engine: one method per member, and the machinery they share.
//!
//! [`Desktop`] is the whole of this crate's behavior. It holds the
//! configuration a command runs under — session identity, tracing, whether
//! physical input is permitted — and exposes one method per member of
//! [`tinydesktop_bus::names::METHODS`]. `tinybus_module` does nothing but adapt
//! those methods to the bus, which is what keeps the feature code testable
//! without a broker.
//!
//! # Why every method returns a response instead of a `Result`
//!
//! A command method returns [`DesktopResponse`] for both outcomes and never
//! fails in the Rust sense. That is deliberate, and it is the same reasoning
//! [`crate::Error`] documents: a stale ref carries a recovery hint, a denied
//! permission carries the setting to change, an ambiguous application name
//! carries the candidates. Those belong in a payload a caller can branch on,
//! not in a `Result::Err` that a bus adapter would flatten to a string.
//!
//! # What happens on each call
//!
//! 1. A platform adapter is constructed. Every adapter is a zero-sized type,
//!    so this costs nothing and avoids holding a platform handle across an
//!    `await` in the bus layer.
//! 2. A [`CommandContext`] is built from this `Desktop`'s configuration. It
//!    carries the session the refs live in and the input policy the command
//!    runs under.
//! 3. The command's [permission need](permission::Need) is checked against the
//!    platform's report, so a command that cannot possibly work fails with
//!    `PERM_DENIED` and the setting to change rather than with whatever the
//!    accessibility API returns for an unauthorized process.
//! 4. The engine runs the command, and its result — value or error — is
//!    wrapped in the envelope.
//!
//! # Files
//!
//! The command methods are split by family across sibling files, each holding
//! one `impl Desktop` block: `observation`, `interaction`, `input`, `apps`,
//! `clipboard`, `notifications`, `waiting`, and `system`. `convert` maps
//! contract payloads onto the engine's argument types, `reply` maps the
//! engine's errors onto the envelope, and `permission` holds the preflight.

mod apps;
mod clipboard;
mod convert;
mod input;
mod interaction;
mod notifications;
mod observation;
mod permission;
mod reply;
mod system;
mod waiting;

use std::path::PathBuf;

use agent_desktop_core::{AppError, PermissionReport, PlatformAdapter, context::CommandContext};
use serde_json::Value;
use tinydesktop_bus::DesktopResponse;

use crate::{Error, Result};
use permission::Need;

/// The desktop automation engine, configured once and called many times.
///
/// A default `Desktop` runs without a named session, without tracing, and
/// headless — meaning ref actions go through accessibility APIs and are
/// blocked from stealing focus, moving the cursor, or touching the pasteboard
/// as a side effect. That is the configuration a background agent wants.
///
/// # Examples
///
/// ```
/// use tinydesktop_desktop::Desktop;
///
/// let desktop = Desktop::new();
/// let reply = desktop.version();
///
/// assert!(reply.ok);
/// assert!(reply.data.is_some());
/// ```
#[derive(Debug, Clone, Default)]
pub struct Desktop {
    session_id: Option<String>,
    trace_path: Option<PathBuf>,
    trace_strict: bool,
    headed: bool,
    motion: MotionProfile,
    pointer: Pointer,
}

/// The virtual mouse every clone of a [`Desktop`] shares: there is one real
/// pointer, so there is one record of where it was left.
#[derive(Debug, Clone)]
pub(crate) struct Pointer(pub(crate) Arc<Mutex<VirtualMouse>>);

impl Pointer {
    fn new(profile: MotionProfile) -> Self {
        Self(Arc::new(Mutex::new(VirtualMouse::new(profile))))
    }
}

impl Default for Pointer {
    fn default() -> Self {
        Self::new(MotionProfile::default())
    }
}

impl Desktop {
    /// Builds a `Desktop` with every setting at its default.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinydesktop_desktop::Desktop;
    /// assert!(Desktop::new().version().ok);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns this `Desktop` bound to the named session.
    ///
    /// A session is where allocated refs live. Two `Desktop` values sharing a
    /// session id can spend each other's refs; two with different sessions
    /// cannot, which is what keeps concurrent runs from resolving each other's
    /// elements.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinydesktop_desktop::Desktop;
    /// let desktop = Desktop::new().with_session("run-42");
    /// assert_eq!(desktop.session_id(), Some("run-42"));
    /// ```
    #[must_use]
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Returns this `Desktop` writing a command trace to `path`.
    ///
    /// A trace records each command and its outcome, which is what makes a
    /// failed run reconstructable afterwards. `strict` additionally fails a
    /// command whose trace could not be written, rather than letting it succeed
    /// with no record — the right setting when the trace is the audit log
    /// rather than a debugging aid.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinydesktop_desktop::Desktop;
    /// let desktop = Desktop::new().with_trace("/tmp/run.jsonl", false);
    /// assert!(desktop.is_tracing());
    /// ```
    #[must_use]
    pub fn with_trace(mut self, path: impl Into<PathBuf>, strict: bool) -> Self {
        self.trace_path = Some(path.into());
        self.trace_strict = strict;
        self
    }

    /// Returns this `Desktop` in headed or headless mode.
    ///
    /// Headed mode lets a ref action take focus and move the real cursor when
    /// the platform needs it to. It makes some interactions work that
    /// otherwise cannot, at the cost of disturbing whoever is at the machine,
    /// so it is off by default and has to be asked for.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinydesktop_desktop::Desktop;
    /// assert!(Desktop::new().with_headed(true).is_headed());
    /// ```
    #[must_use]
    pub fn with_headed(mut self, headed: bool) -> Self {
        self.headed = headed;
        self
    }

    /// Returns this `Desktop` moving the real pointer at `profile` in headed
    /// mode.
    ///
    /// A headed `Desktop` glides the pointer onto an element along a human
    /// path — curved, overshooting and correcting, at Fitts's-law speed —
    /// before clicking, checking, expanding, or collapsing it, so the
    /// application sees the pointer arrive and hover the way it does under a
    /// hand. [`MotionProfile::Instant`] turns that off. Headless mode never
    /// moves the pointer, whatever the profile.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinydesktop_desktop::{Desktop, MotionProfile};
    /// let desktop = Desktop::new().with_motion(MotionProfile::Calm);
    /// assert_eq!(desktop.motion(), MotionProfile::Calm);
    /// assert_eq!(Desktop::new().motion(), MotionProfile::Natural);
    /// ```
    #[must_use]
    pub fn with_motion(mut self, profile: MotionProfile) -> Self {
        self.motion = profile;
        self.pointer = Pointer::new(profile);
        self
    }

    /// The profile the real pointer moves at in headed mode.
    #[must_use]
    pub const fn motion(&self) -> MotionProfile {
        self.motion
    }

    pub(crate) fn pointer(&self) -> &Pointer {
        &self.pointer
    }

    /// Builds a `Desktop` from the configuration blob the module loader
    /// supplies.
    ///
    /// Every field is optional; `null` and `{}` both yield
    /// [`Desktop::default`]. The recognized fields are `session_id` and
    /// `trace_path` (strings), `trace_strict` and `headed` (booleans), and
    /// `motion` (`instant`, `brisk`, `natural`, or `calm`). An
    /// unrecognized field is ignored, so a newer host configuring a field this
    /// version does not know about still loads.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinydesktop_desktop::Desktop;
    /// let desktop = Desktop::from_config(&serde_json::json!({
    ///     "session_id": "run-42",
    ///     "headed": true,
    /// }))?;
    ///
    /// assert_eq!(desktop.session_id(), Some("run-42"));
    /// assert!(desktop.is_headed());
    /// # Ok::<(), tinydesktop_desktop::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::ConfigNotAnObject`] when the blob is neither `null` nor
    /// an object, and [`Error::ConfigFieldType`] when a recognized field holds
    /// the wrong type.
    pub fn from_config(config: &Value) -> Result<Self> {
        if config.is_null() {
            return Ok(Self::default());
        }
        let Some(object) = config.as_object() else {
            return Err(Error::ConfigNotAnObject);
        };

        Ok(Self {
            session_id: text(object.get("session_id"), "session_id")?,
            trace_path: text(object.get("trace_path"), "trace_path")?.map(PathBuf::from),
            trace_strict: flag(object.get("trace_strict"), "trace_strict")?,
            headed: flag(object.get("headed"), "headed")?,
            motion: MotionProfile::default(),
            pointer: Pointer::default(),
        }
        .with_motion(motion(object.get("motion"))?))
    }

    /// The session refs allocated through this `Desktop` belong to.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinydesktop_desktop::Desktop;
    /// assert_eq!(Desktop::new().session_id(), None);
    /// ```
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Whether commands write a trace.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinydesktop_desktop::Desktop;
    /// assert!(!Desktop::new().is_tracing());
    /// ```
    #[must_use]
    pub fn is_tracing(&self) -> bool {
        self.trace_path.is_some()
    }

    /// Where commands write their trace, if anywhere.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::path::Path;
    /// # use tinydesktop_desktop::Desktop;
    /// let desktop = Desktop::new().with_trace("/tmp/run.jsonl", true);
    /// assert_eq!(desktop.trace_path(), Some(Path::new("/tmp/run.jsonl")));
    /// ```
    #[must_use]
    pub fn trace_path(&self) -> Option<&std::path::Path> {
        self.trace_path.as_deref()
    }

    /// Whether ref actions may take focus and move the real cursor.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinydesktop_desktop::Desktop;
    /// assert!(!Desktop::new().is_headed());
    /// ```
    #[must_use]
    pub fn is_headed(&self) -> bool {
        self.headed
    }

    /// Builds the engine context every command runs under.
    fn context(&self) -> std::result::Result<CommandContext, AppError> {
        CommandContext::new(
            self.session_id.clone(),
            self.trace_path.clone(),
            self.trace_strict,
        )
        .map(|context| context.with_headed(self.headed))
    }

    /// Runs `command` under a fresh adapter and context, and wraps whatever it
    /// produces in the envelope.
    ///
    /// This is the one path every member takes. `need` decides whether a
    /// permission report is fetched and checked first: fetching one costs a
    /// round trip to the platform, so a command that needs no permission does
    /// not pay for it.
    fn run<F>(&self, command: &str, need: Need, run: F) -> DesktopResponse
    where
        F: FnOnce(&dyn PlatformAdapter, &CommandContext) -> std::result::Result<Value, AppError>,
    {
        self.run_with_report(command, need, |adapter, context, _report| {
            run(adapter, context)
        })
    }

    /// [`Desktop::run`] for the two members that also read the report itself.
    fn run_with_report<F>(&self, command: &str, need: Need, run: F) -> DesktopResponse
    where
        F: FnOnce(
            &dyn PlatformAdapter,
            &CommandContext,
            &PermissionReport,
        ) -> std::result::Result<Value, AppError>,
    {
        let adapter = platform_adapter();
        let adapter: &dyn PlatformAdapter = &adapter;

        let result = self.context().and_then(|context| {
            let report = permission::report(need, adapter)?;
            permission::preflight(need, &report)?;
            run(adapter, &context, &report)
        });

        reply::envelope(command, result)
    }
}

/// Reads an optional string configuration field.
fn text(value: Option<&Value>, field: &'static str) -> Result<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(str::to_owned)
            .map(Some)
            .ok_or(Error::ConfigFieldType {
                field,
                expected: "a string",
            }),
    }
}

/// Reads the optional `motion` field, defaulting to the natural profile.
fn motion(value: Option<&Value>) -> Result<MotionProfile> {
    let invalid = Error::ConfigFieldType {
        field: "motion",
        expected: "one of instant, brisk, natural, or calm",
    };
    match value {
        None | Some(Value::Null) => Ok(MotionProfile::default()),
        Some(value) => value
            .as_str()
            .and_then(|name| name.parse().ok())
            .ok_or(invalid),
    }
}

/// Reads an optional boolean configuration field, defaulting to `false`.
fn flag(value: Option<&Value>, field: &'static str) -> Result<bool> {
    match value {
        None | Some(Value::Null) => Ok(false),
        Some(value) => value.as_bool().ok_or(Error::ConfigFieldType {
            field,
            expected: "a boolean",
        }),
    }
}

/// The accessibility backend for the platform this module was built for.
///
/// Every adapter is a zero-sized type, so constructing one per command is free
/// and saves the bus layer from holding a platform handle across an `await`.
fn platform_adapter() -> impl PlatformAdapter {
    #[cfg(target_os = "macos")]
    {
        agent_desktop_macos::MacOSAdapter::new()
    }

    #[cfg(target_os = "windows")]
    {
        agent_desktop_windows::WindowsAdapter::new()
    }

    #[cfg(target_os = "linux")]
    {
        agent_desktop_linux::LinuxAdapter::new()
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    compile_error!(
        "tinydesktop needs an accessibility backend; agent-desktop ships one for macOS, Windows, \
         and Linux"
    )
}

#[cfg(test)]
mod test;
