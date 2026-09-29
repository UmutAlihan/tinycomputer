//! What the browser members share: running a held-output call off the
//! dispatch task, and turning a browser result into the member's envelope.

use tinybus::{Error as TinyBusError, Result as TinyBusResult};
use tinycomputer_browser::Browser;
use tinycomputer_bus::DesktopResponse;

use super::DesktopService;

impl DesktopService {
    /// Runs a held-output call on a blocking thread: encoding a chunk of up
    /// to four mebibytes is work the dispatch task should not wait on.
    pub(super) async fn on_outputs<F>(&self, call: F) -> TinyBusResult<DesktopResponse>
    where
        F: FnOnce(&Browser) -> DesktopResponse + Send + 'static,
    {
        let browser = self.browser.clone();
        tokio::task::spawn_blocking(move || call(&browser))
            .await
            .map_err(|error| TinyBusError::failed(format!("output call failed: {error}")))
    }
}

/// A browser call's result as the envelope every member replies with.
///
/// A value that will not serialize is a module fault, not the caller's, and
/// travels as `INTERNAL` like any other.
pub(super) fn browser_reply<T: serde::Serialize>(
    command: &str,
    result: tinycomputer_browser::Result<T>,
) -> DesktopResponse {
    let data = result.and_then(|value| {
        serde_json::to_value(value).map_err(|error| {
            tinycomputer_browser::Error::failed(format!("reply did not serialize: {error}"))
        })
    });
    match data {
        Ok(data) => DesktopResponse::ok(command, data),
        Err(error) => DesktopResponse::err(command, error.envelope()),
    }
}

/// `BrowserOpenSession`'s reply. A timeout there comes before the session
/// exists, so there is nothing to snapshot or inspect: its hint is a plain
/// retry, and the half-launched browser was never handed out.
pub(super) fn opening_reply(
    result: tinycomputer_browser::Result<tinycomputer_browser::SessionInfo>,
) -> DesktopResponse {
    let timed_out = matches!(result, Err(tinycomputer_browser::Error::Timeout { .. }));
    let mut reply = browser_reply("browser-open-session", result);
    if timed_out && let Some(error) = reply.error.as_mut() {
        error.recovery = Some(tinycomputer_bus::RecoveryHint {
            strategy: "retry_original".to_owned(),
            retryable: true,
            requires_fresh_snapshot: false,
            retry_after_ms: None,
        });
    }
    reply
}
