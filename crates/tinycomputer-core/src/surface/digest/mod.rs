//! A screen as a map rather than a list: the agent-friendly view a decision
//! model reads.
//!
//! A raw observation is a flat list of every actionable element in document
//! order. A person reads a screen differently: whatever pops up in front
//! first, then the part of the page the task is about, while the footer, the
//! ads, and the twenty identical "Select" buttons of a result list fade into
//! the background. [`digest`] does that parsing deterministically:
//!
//! - elements are grouped into **regions** by the ancestors they share, and
//!   a region too large to take in is split one level deeper;
//! - a **dialog, sheet, popover, or consent banner** is its own region,
//!   placed first and marked `in_front`;
//! - a **list of repeated cards** stays one region and is shown as one line
//!   per card — its text and its "open this" control — instead of every
//!   element inside every card;
//! - **advertising, sponsored content, footers, and legal text** are marked
//!   as noise and collapsed to one line.
//!
//! [`Digest::render`] spends a byte budget region by region, most relevant
//! first, so a crowded page shows the part that matters in full and the rest
//! as one-line summaries. Every element keeps a short id (`e12`, its index in
//! `Screen::candidates`) that maps straight back to the element. Everything
//! read from the screen is wrapped as `untrusted_accessibility_data`.

mod types;

pub use types::{Digest, Region, RegionKind, Rendering};

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::{Candidate, Screen, element_line, groups, label};

/// Most elements a region holds before it is split one level deeper.
const REGION_SIZE: usize = 24;
/// Deepest ancestor level regions are split on.
const MAX_DEPTH: usize = 10;
/// Cards of a list shown one line each before the rest are counted.
const LIST_CARDS: usize = 12;
/// Longest card line, in characters.
const CARD_CHARS: usize = 160;
/// Example labels a collapsed region names.
const EXAMPLES: usize = 5;
/// Roles whose subtree is in front of the page.
const FRONT_ROLES: &[&str] = &["sheet", "dialog", "alertdialog", "alert", "popover"];
/// Words in a region's labels that mark an overlay a person must deal with.
const FRONT_WORDS: &[&str] = &[
    "cookie",
    "cookies",
    "consent",
    "gdpr",
    "newsletter",
    "subscribe",
    "popup",
    "modal",
    "overlay",
];
/// Words in a region's labels that mark it as noise.
const NOISE_WORDS: &[&str] = &[
    "ads",
    "advert",
    "advertisement",
    "advertising",
    "sponsored",
    "promoted",
    "promotion",
    "footer",
    "contentinfo",
    "copyright",
];

/// Parses `screen` into regions.
#[must_use]
pub fn digest(screen: &Screen) -> Digest {
    let mut front: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut front_order = Vec::new();
    let mut rest = Vec::new();
    for (index, candidate) in screen.candidates.iter().enumerate() {
        if let Some(overlay) = overlay_of(candidate) {
            if !front.contains_key(&overlay) {
                front_order.push(overlay.clone());
            }
            front.entry(overlay).or_default().push(index);
        } else {
            rest.push(index);
        }
    }
    let mut regions = Vec::new();
    for overlay in front_order {
        let members = front.remove(&overlay).unwrap_or_default();
        regions.push(Region {
            id: String::new(),
            name: region_name(screen, &members),
            kind: RegionKind::Front,
            members,
            list: None,
        });
    }
    let mut content = Vec::new();
    split(screen, rest, 0, &mut content);
    regions.extend(content);
    for (position, region) in regions.iter_mut().enumerate() {
        region.id = format!("r{}", position + 1);
    }
    Digest { regions }
}

impl Digest {
    /// The region `index` (into `Screen::candidates`) belongs to.
    #[must_use]
    pub fn region_of(&self, index: usize) -> Option<&Region> {
        self.regions
            .iter()
            .find(|region| region.members.contains(&index))
    }

    /// The regions in front of the page.
    pub fn front(&self) -> impl Iterator<Item = &Region> {
        self.regions
            .iter()
            .filter(|region| region.kind == RegionKind::Front)
    }

    /// What the page is made of, ignoring its contents: the regions' names
    /// and kinds. It changes when a dialog opens or the page moves on, and
    /// not when a field is typed into, so an attention pass keyed on it is
    /// asked again only when the page's shape changed.
    #[must_use]
    pub fn layout(&self) -> String {
        self.regions
            .iter()
            .map(|region| format!("{:?}:{}", region.kind, region.name))
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Every element index, the most relevant region's first: in front,
    /// then by relevance, then in reading order; distraction and noise last.
    #[must_use]
    pub fn ranked(&self, rendering: &Rendering<'_>) -> Vec<usize> {
        self.ordered(rendering)
            .into_iter()
            .flat_map(|region| region.members.iter().copied())
            .collect()
    }

    /// Renders the digest as the `screen` field of a request.
    ///
    /// Regions are spent against `rendering.budget` in [`Digest::ranked`]
    /// order: one that fits is shown in full, one that does not — and any
    /// noise or distraction the relevance does not rescue — is collapsed to
    /// a line naming its size and a few of its elements.
    #[must_use]
    pub fn render(&self, screen: &Screen, rendering: &Rendering<'_>) -> Value {
        let mut spent = 0;
        let mut in_front = Vec::new();
        let mut shown = Vec::new();
        let mut collapsed = Vec::new();
        for region in self.ordered(rendering) {
            let lines = Self::lines(screen, region, rendering.include_values);
            let cost = lines.iter().map(|line| line.len() + 4).sum::<usize>() + 40;
            let muted = Self::muted(region, rendering);
            if muted || spent + cost > rendering.budget {
                let summary = summary(screen, region);
                spent += summary.len() + 4;
                collapsed.push(summary);
                continue;
            }
            spent += cost;
            let mut entry = json!({"id": region.id, "region": region.name, "elements": lines});
            if let Some(relevance) = rendering
                .relevance
                .and_then(|relevance| relevance.get(&region.id))
            {
                entry["relevance"] = json!((relevance * 100.0).round() / 100.0);
            }
            if region.kind == RegionKind::Front {
                in_front.push(entry);
            } else {
                shown.push(entry);
            }
        }
        let mut view = serde_json::Map::new();
        if !in_front.is_empty() {
            view.insert("in_front".to_owned(), Value::Array(in_front));
        }
        view.insert("regions".to_owned(), Value::Array(shown));
        if !collapsed.is_empty() {
            view.insert("collapsed".to_owned(), json!(collapsed));
        }
        json!({"untrusted_accessibility_data": view})
    }

    /// The regions in the order they are shown and ranked.
    fn ordered(&self, rendering: &Rendering<'_>) -> Vec<&Region> {
        let mut regions = self.regions.iter().enumerate().collect::<Vec<_>>();
        regions.sort_by(|(left_at, left), (right_at, right)| {
            let key = |region: &Region| {
                (
                    region.kind != RegionKind::Front,
                    Self::muted(region, rendering),
                )
            };
            key(left)
                .cmp(&key(right))
                .then_with(|| {
                    relevance(right, rendering).total_cmp(&relevance(left, rendering))
                })
                .then_with(|| left_at.cmp(right_at))
        });
        regions.into_iter().map(|(_, region)| region).collect()
    }

    /// Whether `region` is collapsed whatever the budget: noise or a
    /// distraction that no relevance answer rescued.
    fn muted(region: &Region, rendering: &Rendering<'_>) -> bool {
        if region.kind == RegionKind::Front {
            return false;
        }
        let distracting = region.kind == RegionKind::Noise
            || rendering
                .distractions
                .is_some_and(|distractions| distractions.contains(&region.id));
        let rescued = rendering
            .relevance
            .and_then(|relevance| relevance.get(&region.id))
            .is_some_and(|relevance| *relevance >= 0.5);
        distracting && !rescued
    }

    /// The lines a region is shown as: one per element, or one per card of
    /// a list.
    fn lines(screen: &Screen, region: &Region, include_values: bool) -> Vec<String> {
        if let Some((depth, parent)) = &region.list {
            return card_lines(screen, *depth, parent);
        }
        region
            .members
            .iter()
            .filter_map(|index| {
                let node = screen.candidates.get(*index)?;
                Some(format!("e{index} {}", element_line(node, include_values)))
            })
            .collect()
    }
}

/// A region's relevance under `rendering`: its answer, or 0.5 for content
/// and 0 for noise when there is none.
fn relevance(region: &Region, rendering: &Rendering<'_>) -> f64 {
    rendering
        .relevance
        .and_then(|relevance| relevance.get(&region.id).copied())
        .unwrap_or(if region.kind == RegionKind::Noise {
            0.0
        } else {
            0.5
        })
}

/// One line per card of the list under `parent`, then a count of the rest.
fn card_lines(screen: &Screen, depth: usize, parent: &[String]) -> Vec<String> {
    let cards = groups::cards_at(screen, depth, parent);
    let index_of = |target: &Candidate| {
        screen
            .candidates
            .iter()
            .position(|candidate| candidate.ref_id == target.ref_id)
    };
    let mut lines = cards
        .iter()
        .take(LIST_CARDS)
        .enumerate()
        .map(|(position, card)| {
            let text = clip(&card.fields.join(" · "), CARD_CHARS);
            match card.primary.as_ref().and_then(|primary| {
                index_of(primary).map(|index| format!("e{index} {}", label(primary)))
            }) {
                Some(control) => format!("card {}: {text} → {control}", position + 1),
                None => format!("card {}: {text}", position + 1),
            }
        })
        .collect::<Vec<_>>();
    if cards.len() > LIST_CARDS {
        lines.push(format!(
            "and {} more cards like these",
            cards.len() - LIST_CARDS
        ));
    }
    lines
}

/// One line for a collapsed region: its id, where it is, how many elements,
/// and a few of them.
fn summary(screen: &Screen, region: &Region) -> String {
    let examples = region
        .members
        .iter()
        .filter_map(|index| screen.candidates.get(*index))
        .take(EXAMPLES)
        .map(|node| clip(&label(node), 40))
        .collect::<Vec<_>>();
    let kind = match region.kind {
        RegionKind::Noise => ", likely noise",
        _ => "",
    };
    format!(
        "{} {}: {} elements{kind}, e.g. {}",
        region.id,
        region.name,
        region.members.len(),
        examples.join(", ")
    )
}

/// Splits `members` into regions of at most [`REGION_SIZE`], one ancestor
/// level at a time, keeping a list of repeated cards whole and apart from
/// whatever sits beside it.
fn split(screen: &Screen, members: Vec<usize>, level: usize, out: &mut Vec<Region>) {
    if members.is_empty() {
        return;
    }
    if let Some(list) = list_at(screen, &members, level) {
        out.push(region(screen, members, Some(list)));
        return;
    }
    if level >= MAX_DEPTH || (members.len() <= REGION_SIZE && !holds_a_list(screen, &members)) {
        out.push(region(screen, members, None));
        return;
    }
    let mut groups: Vec<(Option<String>, Vec<usize>)> = Vec::new();
    for index in members {
        let key = screen.candidates[index].path.get(level).cloned();
        match groups.iter_mut().find(|(existing, _)| *existing == key) {
            Some((_, group)) => group.push(index),
            None => groups.push((key, vec![index])),
        }
    }
    for (_, group) in groups {
        split(screen, group, level + 1, out);
    }
}

/// The list `members` form at `level`: every member sits under the same
/// ancestors and then under one of at least two ordinal containers
/// (`listitem #3`) there.
fn list_at(screen: &Screen, members: &[usize], level: usize) -> Option<(usize, Vec<String>)> {
    let first = &screen.candidates[*members.first()?].path;
    let parent = first.get(..level)?;
    let mut containers = Vec::new();
    for index in members {
        let path = &screen.candidates[*index].path;
        let container = path.get(level)?;
        if path.get(..level)? != parent || groups::ordinal(container).is_none() {
            return None;
        }
        if !containers.contains(&container) {
            containers.push(container);
        }
    }
    (containers.len() >= 2).then(|| (level, parent.to_vec()))
}

/// Whether two or more of `members` sit in different ordinal containers
/// under the same ancestors: a list that needs a region of its own.
fn holds_a_list(screen: &Screen, members: &[usize]) -> bool {
    let mut seen: BTreeMap<(usize, &[String]), &String> = BTreeMap::new();
    for index in members {
        let path = &screen.candidates[*index].path;
        for (level, label) in path.iter().enumerate() {
            if groups::ordinal(label).is_none() {
                continue;
            }
            match seen.get(&(level, &path[..level])) {
                Some(other) if *other != label => return true,
                Some(_) => {}
                None => {
                    seen.insert((level, &path[..level]), label);
                }
            }
        }
    }
    false
}

fn region(screen: &Screen, members: Vec<usize>, list: Option<(usize, Vec<String>)>) -> Region {
    let name = region_name(screen, &members);
    let noisy = members
        .first()
        .map(|index| &screen.candidates[*index].path)
        .is_some_and(|path| {
            // The root is the window or page itself: its title says nothing
            // about which part of it is noise.
            path.iter()
                .skip(1)
                .any(|label| words(label).any(|word| NOISE_WORDS.contains(&word.as_str())))
        });
    Region {
        id: String::new(),
        name,
        kind: if noisy {
            RegionKind::Noise
        } else {
            RegionKind::Content
        },
        members,
        list,
    }
}

/// The last two labels every member's path shares, or `top level`.
fn region_name(screen: &Screen, members: &[usize]) -> String {
    let paths = members
        .iter()
        .filter_map(|index| screen.candidates.get(*index))
        .map(|candidate| &candidate.path)
        .collect::<Vec<_>>();
    let Some(first) = paths.first() else {
        return "top level".to_owned();
    };
    let shared = (0..first.len())
        .take_while(|level| paths.iter().all(|path| path.get(*level) == first.get(*level)))
        .count();
    if shared == 0 {
        return "top level".to_owned();
    }
    first[shared.saturating_sub(2)..shared].join(" > ")
}

/// The overlay `candidate` sits inside, named by its ancestor labels down to
/// the overlay, or `None` when it is part of the page itself.
fn overlay_of(candidate: &Candidate) -> Option<String> {
    candidate
        .path
        .iter()
        .enumerate()
        .position(|(level, label)| {
            let role = label.split(' ').next().unwrap_or_default().to_lowercase();
            // A root titled "Newsletter" is the page, not something over it.
            FRONT_ROLES.contains(&role.as_str())
                || (level > 0 && words(label).any(|word| FRONT_WORDS.contains(&word.as_str())))
        })
        .map(|at| candidate.path[at.saturating_sub(1)..=at].join(" > "))
}

fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}

fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut clipped = text.chars().take(limit).collect::<String>();
    clipped.push('…');
    clipped
}

#[cfg(test)]
mod test;
