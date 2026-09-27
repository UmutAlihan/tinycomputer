//! The pasteboard members.

use agent_desktop_core::commands::{clipboard_clear, clipboard_get, clipboard_set};
use tinydesktop_bus::{ClipboardGetRequest, ClipboardSetRequest, DesktopResponse};

use super::{Desktop, Need, convert};

impl Desktop {
    /// Reads the pasteboard.
    ///
    /// An empty pasteboard is not a failure: the reply reports `found: false`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinydesktop_desktop::{ClipboardGetRequest, Desktop};
    /// let reply = Desktop::new().clipboard_get(ClipboardGetRequest::default());
    /// assert_eq!(reply.command, "clipboard-get");
    /// ```
    #[must_use]
    pub fn clipboard_get(&self, request: ClipboardGetRequest) -> DesktopResponse {
        self.run("clipboard-get", Need::Nothing, |adapter, context| {
            clipboard_get::execute(
                clipboard_get::ClipboardGetArgs {
                    format: request.format.map(convert::clipboard_format),
                    out: convert::path(request.out),
                },
                adapter,
                context,
            )
        })
    }

    /// Writes the pasteboard.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinydesktop_desktop::{ClipboardSetRequest, Desktop};
    /// let reply = Desktop::new().clipboard_set(ClipboardSetRequest::text("hello"));
    /// assert_eq!(reply.command, "clipboard-set");
    /// ```
    #[must_use]
    pub fn clipboard_set(&self, request: ClipboardSetRequest) -> DesktopResponse {
        self.run("clipboard-set", Need::Nothing, |adapter, _context| {
            clipboard_set::execute(
                clipboard_set::ClipboardSetArgs {
                    text: request.text,
                    image: convert::path(request.image),
                    file_urls: request.file_urls,
                },
                adapter,
            )
        })
    }

    /// Empties the pasteboard.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinydesktop_desktop::Desktop;
    /// let reply = Desktop::new().clipboard_clear();
    /// assert_eq!(reply.command, "clipboard-clear");
    /// ```
    #[must_use]
    pub fn clipboard_clear(&self) -> DesktopResponse {
        self.run("clipboard-clear", Need::Nothing, |adapter, _context| {
            clipboard_clear::execute(adapter)
        })
    }
}
