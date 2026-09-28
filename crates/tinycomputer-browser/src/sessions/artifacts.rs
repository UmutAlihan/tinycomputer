//! What sessions leave behind: held outputs and downloads.

use serde_json::{Value, json};
use tinycomputer_bus::browser::{
    DownloadId, DownloadInfo, DownloadState, DownloadWaitRequest, OutputChunk, OutputId, SessionId,
};

use super::Browser;
#[cfg(doc)]
use crate::error::Error;
use crate::error::Result;

impl Browser {
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
}
