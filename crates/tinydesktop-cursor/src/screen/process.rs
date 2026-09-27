//! The overlay helper process: `tinydesktop-cursor-overlay`, fed commands on
//! its standard input.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};

use super::OverlaySink;
use crate::protocol::OverlayCommand;

/// The helper's executable name, without a platform suffix.
pub const HELPER_NAME: &str = "tinydesktop-cursor-overlay";

/// An environment variable naming the helper's path, for hosts that install
/// it somewhere of their own.
pub const HELPER_ENV: &str = "TINYDESKTOP_CURSOR_OVERLAY";

/// Commands waiting for the helper. A glide is a few kilobytes; if the
/// helper falls this far behind, newer glides are dropped rather than
/// holding up the action that sent them.
const QUEUE: usize = 8;

/// A running overlay helper. Dropping it ends the helper.
///
/// Commands are written to the helper by a background thread, so sending
/// never blocks on the helper's pipe.
#[derive(Debug)]
pub struct ProcessOverlay {
    child: Child,
    queue: SyncSender<String>,
    broken: Arc<AtomicBool>,
}

impl ProcessOverlay {
    /// Starts the helper at `path`, or wherever [`ProcessOverlay::locate`]
    /// finds it.
    ///
    /// # Errors
    ///
    /// When the helper cannot be found or started.
    pub fn spawn(path: Option<&Path>) -> std::io::Result<Self> {
        let path = path
            .map(Path::to_path_buf)
            .or_else(Self::locate)
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "no cursor overlay helper")
            })?;
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("the helper has no standard input"))?;
        let (queue, lines) = sync_channel::<String>(QUEUE);
        let broken = Arc::new(AtomicBool::new(false));
        let failed = broken.clone();
        std::thread::Builder::new()
            .name("tinydesktop-cursor-overlay".into())
            .spawn(move || {
                for line in lines {
                    if stdin
                        .write_all(line.as_bytes())
                        .and_then(|()| stdin.flush())
                        .is_err()
                    {
                        failed.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            })?;
        Ok(Self {
            child,
            queue,
            broken,
        })
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
    /// Queues `command` for the helper without waiting for it. A full queue
    /// drops the command; a helper that has gone is an error, so the cursor
    /// stops drawing.
    fn send(&mut self, command: &OverlayCommand) -> std::io::Result<()> {
        if self.broken.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("the cursor overlay has gone"));
        }
        match self.queue.try_send(command.to_line()) {
            Ok(()) | Err(TrySendError::Full(_)) => Ok(()),
            Err(TrySendError::Disconnected(_)) => {
                Err(std::io::Error::other("the cursor overlay has gone"))
            }
        }
    }
}

impl Drop for ProcessOverlay {
    fn drop(&mut self) {
        let _killed = self.child.kill();
        let _reaped = self.child.wait();
    }
}
