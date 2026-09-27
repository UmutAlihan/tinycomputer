//! Payload types for screenshots and the outputs they are held as.

use serde::{Deserialize, Serialize};

use crate::Target;

/// The identity of one held output.
///
/// A newtype for the same reason [`crate::SessionId`] is one: both are opaque
/// strings, and swapping them is otherwise a runtime error rather than a
/// compile-time one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OutputId(String);

impl OutputId {
    /// Wraps `id` as an output identity.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinybrowser_bus::OutputId;
    /// assert_eq!(OutputId::new("o-1").as_str(), "o-1");
    /// ```
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The identity as it appears on the wire.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for OutputId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for OutputId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for OutputId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

/// The encoding a screenshot is captured in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    /// Lossless. Correct for a screenshot a model will read text from, and the
    /// default for that reason.
    #[default]
    Png,
    /// Lossy, and much smaller. Correct for a long full-page capture where the
    /// question is layout rather than legibility.
    Jpeg,
    /// Lossy, smaller again, and not universally readable downstream.
    Webp,
}

impl ImageFormat {
    /// The media type of an image in this format.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinybrowser_bus::ImageFormat;
    /// assert_eq!(ImageFormat::Png.media_type(), "image/png");
    /// ```
    #[must_use]
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
        }
    }
}

/// What to capture.
///
/// The default is the viewport as a PNG, which is what a model looking at a page
/// wants: lossless enough to read text out of, and bounded by the viewport
/// rather than by however long the document happens to be.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScreenshotRequest {
    /// Capture just this element. Defaults to the viewport.
    pub target: Option<Target>,
    /// Capture the whole scrollable document rather than the viewport.
    pub full_page: bool,
    /// The encoding to capture in.
    pub format: ImageFormat,
    /// Quality from 1 to 100, for the lossy formats. Ignored for
    /// [`ImageFormat::Png`].
    pub quality: Option<u8>,
}

/// A handle to an image the module is holding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputRef {
    /// The identity to read and release it by.
    pub id: OutputId,
    /// Its total size in bytes.
    ///
    /// This is a number the module sent: a host sizing a buffer from it should
    /// treat it as a bound to check against rather than one to trust, because a
    /// wrong value turns into a failed allocation, which aborts a process rather
    /// than returning an error.
    pub total_bytes: u64,
    /// The lowercase hex SHA-256 of the complete output.
    pub sha256: String,
    /// The media type of the bytes, from [`ImageFormat::media_type`].
    pub media_type: String,
    /// The captured image's pixel width.
    pub width: u32,
    /// The captured image's pixel height.
    pub height: u32,
}

/// One piece of a held output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputChunk {
    /// The output this came from.
    pub id: OutputId,
    /// The byte offset this chunk starts at.
    pub offset: u64,
    /// The bytes, base64 with standard alphabet and padding.
    ///
    /// Base64 rather than a JSON array of numbers: the array form costs roughly
    /// four bytes per byte and parses far slower, and a bus frame is the scarce
    /// resource here.
    pub data: String,
    /// Whether this chunk reaches the end of the output.
    pub eof: bool,
}
