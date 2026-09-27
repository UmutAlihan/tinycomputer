//! Payloads for `ClipboardGet` and `ClipboardSet`.

use serde::{Deserialize, Serialize};

use crate::vocabulary::ClipboardFormat;

/// The argument to [`crate::names::methods::CLIPBOARD_GET`].
///
/// An image is never inlined: the engine writes it to a temporary file and the
/// reply carries the path. Supply `out` to choose that path.
///
/// A pasteboard that holds nothing in the requested format is not an error —
/// the reply reports `found: false`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipboardGetRequest {
    /// The flavor to read. Absent means [`ClipboardFormat::Text`].
    pub format: Option<ClipboardFormat>,
    /// Where to write an image result instead of a temporary file.
    pub out: Option<String>,
}

/// The argument to [`crate::names::methods::CLIPBOARD_SET`].
///
/// Exactly one of the three fields carries the content. Supplying none, or
/// more than one, is an `INVALID_ARGS` error.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipboardSetRequest {
    /// Text to place on the pasteboard.
    pub text: Option<String>,
    /// The path of an image to place on the pasteboard.
    pub image: Option<String>,
    /// File URLs to place on the pasteboard.
    pub file_urls: Vec<String>,
}

impl ClipboardSetRequest {
    /// Builds a request placing `text` on the pasteboard.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_bus::ClipboardSetRequest;
    /// let request = ClipboardSetRequest::text("hello");
    /// assert_eq!(request.text.as_deref(), Some("hello"));
    /// assert!(request.file_urls.is_empty());
    /// ```
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            ..Self::default()
        }
    }
}
