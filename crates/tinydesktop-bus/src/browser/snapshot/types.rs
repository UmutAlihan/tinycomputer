//! Payload types for the accessibility snapshot.

use serde::{Deserialize, Serialize};

/// What to include in a snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapshotRequest {
    /// Snapshot only the subtree under the first element matching this CSS
    /// selector.
    pub selector: Option<String>,
    /// Keep only elements an agent can act on — links, buttons, fields, and
    /// their labels — dropping the prose between them. Roughly a tenth the size,
    /// and the right default for "what can I click here".
    pub interactive_only: bool,
    /// Drop the structural scaffolding — generic containers, groups, rows that
    /// carry no name — collapsing their children into the parent.
    pub compact: bool,
    /// Stop descending after this many levels.
    pub depth: Option<u32>,
    /// Annotate links with their resolved destination.
    pub include_urls: bool,
    /// Truncate the rendered tree to this many characters. Defaults to 200,000.
    pub max_chars: usize,
}

impl Default for SnapshotRequest {
    fn default() -> Self {
        Self {
            selector: None,
            interactive_only: false,
            compact: false,
            depth: None,
            include_urls: false,
            max_chars: 200_000,
        }
    }
}

impl SnapshotRequest {
    /// A request for the interactive elements alone.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinybrowser_bus::SnapshotRequest;
    /// assert!(SnapshotRequest::interactive().interactive_only);
    /// ```
    #[must_use]
    pub fn interactive() -> Self {
        Self {
            interactive_only: true,
            ..Self::default()
        }
    }
}

/// One addressable element in a snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementRef {
    /// The ref, without its `@`. Pass it to [`crate::Target::reference`].
    pub id: String,
    /// The element's accessibility role.
    pub role: String,
    /// Its accessible name, empty when it has none.
    pub name: String,
}

/// The rendered accessibility tree of a page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// The URL the snapshot was taken at.
    pub url: String,
    /// The document title.
    pub title: String,
    /// Which generation of refs this snapshot minted.
    ///
    /// A ref is only valid for the generation that minted it, and the counter
    /// moves on for every snapshot *and* every navigation — a navigation
    /// replaces the document, so the refs naming its nodes are as dead as if a
    /// new snapshot had replaced them. It is therefore a monotonic counter
    /// rather than a count of snapshots: the first snapshot after a navigation
    /// will not be number one.
    ///
    /// Reporting it lets a host say "this ref is two generations old" rather
    /// than discovering it by acting on the wrong element.
    pub sequence: u64,
    /// The tree, as indented text with `@ref` markers.
    pub tree: String,
    /// Every ref in `tree`, in the order it appears.
    pub refs: Vec<ElementRef>,
    /// Whether `max_chars` cut the tree short.
    pub truncated: bool,
}
