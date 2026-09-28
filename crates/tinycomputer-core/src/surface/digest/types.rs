//! The digest's types: regions of a screen, what kind each is, and how a
//! rendering is budgeted.

use std::collections::{BTreeMap, BTreeSet};

/// A screen, parsed into regions a reader can take in at a glance.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Digest {
    /// Every region: those in front first, then the rest in reading order.
    pub regions: Vec<Region>,
}

/// One region of a screen: a group of elements that sit under the same
/// ancestors, such as a toolbar, a form, a result list, or a dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    /// A short id, `r1`, `r2`, …, stable within one digest.
    pub id: String,
    /// Where the region sits, as its last two ancestor labels.
    pub name: String,
    /// Whether it is in front, ordinary content, or likely noise.
    pub kind: RegionKind,
    /// Indices of its elements in `Screen::candidates`, in reading order.
    pub members: Vec<usize>,
    /// For a list of repeated cards: the depth of the card containers in an
    /// element's path, and the path above them.
    pub list: Option<(usize, Vec<String>)>,
}

/// What a region is, before anyone has been asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RegionKind {
    /// A dialog, sheet, popover, or banner overlaying the page: whatever is
    /// in it is what a person would have to deal with first.
    Front,
    /// Ordinary content.
    Content,
    /// Advertising, sponsored content, a footer, or legal boilerplate: shown
    /// collapsed unless a survey says it matters.
    Noise,
}

/// How a digest is rendered for one request.
#[derive(Debug, Clone, Default)]
pub struct Rendering<'a> {
    /// Whether field values may be shown.
    pub include_values: bool,
    /// Most bytes the rendered elements may take; regions that do not fit
    /// are collapsed to one line.
    pub budget: usize,
    /// How much each region matters to the step, 0 to 1, by region id;
    /// regions not listed count as 0.5, and noise as 0.
    pub relevance: Option<&'a BTreeMap<String, f64>>,
    /// Regions judged to be distraction, by id: collapsed and ranked last.
    pub distractions: Option<&'a BTreeSet<String>>,
}
