//! Rendering a digest within a byte budget, most relevant region first, and
//! looking elements up by region.

use serde_json::{Value, json};

use super::{CARD_CHARS, COLLAPSED_SLACK, EXAMPLES, LIST_CARDS, SUMMARY_CHARS};
use super::{Candidate, Screen, element_line, groups, label};
use super::{Digest, Region, RegionKind, Rendering};

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
        let mut omitted = 0;
        for region in self.ordered(rendering) {
            let lines = Self::lines(screen, region, rendering.include_values);
            let cost = lines.iter().map(|line| line.len() + 4).sum::<usize>() + 40;
            let muted = Self::muted(region, rendering);
            if muted || spent + cost > rendering.budget {
                // A collapsed summary still spends the same budget: bounded
                // in length on its own, and stopped once summaries alone
                // would run the total well past it, so a page split into
                // many small regions cannot inflate the digest without
                // limit.
                if spent > rendering.budget + COLLAPSED_SLACK {
                    omitted += 1;
                    continue;
                }
                let summary = clip(&summary(screen, region), SUMMARY_CHARS);
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
        if omitted > 0 {
            collapsed.push(format!("and {omitted} more regions not shown"));
        }
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
                .then_with(|| relevance(right, rendering).total_cmp(&relevance(left, rendering)))
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
            return card_lines(screen, *depth, parent, include_values);
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
/// `include_values` gates a card's field content exactly as
/// [`super::screen::element_line`] gates an ordinary element's held value.
fn card_lines(
    screen: &Screen,
    depth: usize,
    parent: &[String],
    include_values: bool,
) -> Vec<String> {
    let cards = groups::cards_at(screen, depth, parent, include_values);
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

pub(super) fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut clipped = text.chars().take(limit).collect::<String>();
    clipped.push('…');
    clipped
}
