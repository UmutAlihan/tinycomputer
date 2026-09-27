//! The overlay helper process: `tinydesktop-cursor-overlay`, fed commands on
//! its standard input.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

use super::OverlaySink;
use crate::protocol::OverlayCommand;

/// The helper's executable name, without a platform suffix.
pub const HELPER_NAME: &str = "tinydesktop-cursor-overlay";

/// An environment variable naming the helper's path, for hosts that install
/// it somewhere of their own.
pub const HELPER_ENV: &str = "TINYDESKTOP_CURSOR_OVERLAY";

/// A running overlay helper. Dropping it ends the helper.
#[derive(Debug)]
pub struct ProcessOverlay {
    child: Child,
    stdin: ChildStdin,
}

impl ProcessOverlay {
    /// Starts the helper at `path`, or wherever [`ProcessOverlay::locate`]
    /// finds it.
    ///
    /// # Errors
    ///
    /// When the helper cannot be found or started.
    pub fn spawn(path: Option<&Path>) -> std::io::Result<Self> {
        let path = path.map(Path::to_path_buf).or_else(Self::locate).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "no cursor overlay helper")
        })?;
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("the helper has no standard input"))?;
        Ok(Self { child, stdin })
    }

    /// Where the helper is: [`HELPER_ENV`] when set, else beside the running
    /// executable, else [`HELPER_NAME`] on `PATH`.
    #[must_use]
    pub fn locate() -> Option<PathBuf> {
        let file = format!("{HELPER_NAME}{}", std::env::consts::EXE_SUFFIX);
        std::env::var_os(HELPER_ENV)
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .or_else(|| {
                std::env::current_exe()
                    .ok()
                    .and_then(|exe| exe.parent().map(|dir| dir.join(&file)))
                    .filter(|path| path.is_file())
            })
            .or_else(|| {
                std::env::var_os("PATH").and_then(|paths| {
                    std::env::split_paths(&paths)
                        .map(|dir| dir.join(&file))
                        .find(|path| path.is_file())
                })
            })
    }
}

impl OverlaySink for ProcessOverlay {
    fn send(&mut self, command: &OverlayCommand) -> std::io::Result<()> {
        self.stdin.write_all(command.to_line().as_bytes())?;
        self.stdin.flush()
    }
}

impl Drop for ProcessOverlay {
    fn drop(&mut self) {
        let _killed = self.child.kill();
        let _reaped = self.child.wait();
    }
}
