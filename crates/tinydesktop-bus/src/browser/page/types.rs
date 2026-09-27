//! Payload types for navigation, extraction, and evaluation.

use serde::{Deserialize, Serialize};

/// How far a navigation waits before the module calls it settled.
///
/// The choice is a trade between a page that is merely reachable and one that is
/// actually usable. There is no single right answer, which is why it is on the
/// request rather than hard-coded: a login redirect wants [`WaitUntil::Load`],
/// and a single-page application that streams its content only ever settles at
/// [`WaitUntil::NetworkIdle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitUntil {
    /// Return as soon as the browser has committed to the navigation. The page
    /// may still be blank.
    Commit,
    /// Wait for `DOMContentLoaded`: the document is parsed, subresources may
    /// still be in flight.
    DomContentLoaded,
    /// Wait for the `load` event.
    Load,
    /// Wait until the page has made no network request for a short quiet period,
    /// or the deadline expires — whichever comes first. A page that polls in the
    /// background never goes idle, so this is bounded rather than absolute.
    NetworkIdle,
}

impl Default for WaitUntil {
    /// [`WaitUntil::Load`]: the point at which most pages are both rendered and
    /// interactive, and the one a human would call "loaded".
    fn default() -> Self {
        Self::Load
    }
}

/// Where to send the session's active page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NavigateRequest {
    /// The destination. A bare host such as `example.com` is read as `https://`,
    /// matching what an operator would type; anything else must carry its
    /// scheme, and only `http` and `https` are accepted.
    pub url: String,
    /// How far to wait before returning.
    pub wait_until: WaitUntil,
    /// Deadline in milliseconds. Falls back to the session's default when absent.
    pub timeout_ms: Option<u64>,
}

impl NavigateRequest {
    /// A request for `url` with the default wait and the session's deadline.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinybrowser_bus::{NavigateRequest, WaitUntil};
    /// let request = NavigateRequest::new("https://example.com");
    /// assert_eq!(request.wait_until, WaitUntil::Load);
    /// ```
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Self::default()
        }
    }
}

/// Where the session's active page currently is.
///
/// Returned by every member that can move the page, so a host never has to make
/// a second call to find out where an action left it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageState {
    /// The URL after any redirect.
    pub url: String,
    /// The document title, empty if it has none.
    pub title: String,
    /// The HTTP status of the main document, absent when the page was not
    /// reached over HTTP — `about:blank`, a `data:` URL, or a same-document
    /// navigation that issued no request.
    pub status: Option<u16>,
}

impl PageState {
    /// A state for `url` with no title and no status.
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            title: String::new(),
            status: None,
        }
    }
}

/// The shape a page is extracted into.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadFormat {
    /// The rendered text of the page, with the chrome — navigation, scripts,
    /// styles, hidden nodes — dropped.
    Text,
    /// The same content as [`ReadFormat::Text`], keeping headings, links, lists,
    /// and code blocks as Markdown. This is what a model reads best, and the
    /// default for that reason.
    #[default]
    Markdown,
    /// The live serialized DOM, after scripts have run.
    ///
    /// This is not the response body: it is what the page became. It is here for
    /// a host that needs to parse structure the other two formats discard, and
    /// it is by far the most expensive of the three.
    Html,
}

/// A request to read the active page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadRequest {
    /// The shape to extract into.
    pub format: ReadFormat,
    /// Restrict extraction to the first element matching this CSS selector.
    pub selector: Option<String>,
    /// Truncate the extracted content to this many characters. Defaults to
    /// 200,000 — comfortably inside the bus frame limit, and already far more
    /// than a model will read.
    pub max_chars: usize,
}

impl Default for ReadRequest {
    fn default() -> Self {
        Self {
            format: ReadFormat::default(),
            selector: None,
            max_chars: 200_000,
        }
    }
}

/// The extracted page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageText {
    /// The URL the content came from.
    pub url: String,
    /// The document title.
    pub title: String,
    /// The shape it was extracted into.
    pub format: ReadFormat,
    /// The content itself.
    pub content: String,
    /// Whether `max_chars` cut the content short. A host that shows a model
    /// truncated content without saying so invites it to conclude the rest of
    /// the page does not exist.
    pub truncated: bool,
}

/// JavaScript to run in the active page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EvaluateRequest {
    /// The expression to evaluate. Its completion value is what comes back.
    pub expression: String,
    /// Await the result when it is a promise. Defaults to `true`, because an
    /// unawaited promise serializes as an empty object and looks like a bug in
    /// the module rather than in the expression.
    pub await_promise: bool,
    /// Deadline in milliseconds. Falls back to the session's default when absent.
    pub timeout_ms: Option<u64>,
}

impl Default for EvaluateRequest {
    fn default() -> Self {
        Self {
            expression: String::new(),
            await_promise: true,
            timeout_ms: None,
        }
    }
}

impl EvaluateRequest {
    /// A request evaluating `expression` with the defaults.
    #[must_use]
    pub fn new(expression: impl Into<String>) -> Self {
        Self {
            expression: expression.into(),
            ..Self::default()
        }
    }
}
