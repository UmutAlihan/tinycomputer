//! Repeated cards on a screen — search results, listings, inboxes — as
//! records a task can rank.
//!
//! A surface labels repeated containers with an ordinal (`listitem #3`), so
//! every node under one card shares that card's label in its path. The list
//! is the parent under which the most same-role containers repeat; each
//! container is one record, its text is the record's fields, and its most
//! "open this" control is how the record is chosen.
//!
//! A desktop tree labels no containers that way: a chat's messages are a run
//! of sibling text elements under one parent. When nothing on a screen
//! repeats by ordinal, each run of `MIN_FLAT_ITEMS` or more same-role leaf
//! siblings is a list instead, one record per element.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{Candidate, Screen};

/// Words that mark the control that opens or selects a card.
const OPENERS: &[&str] = &[
    "select", "book", "choose", "view", "details", "continue", "reserve", "deal", "see",
];

/// How many same-role leaf siblings a parent must hold to be read as a list
/// when nothing repeats by ordinal: two could be a label and its value;
/// three is a run.
const MIN_FLAT_ITEMS: usize = 3;

/// One repeated card.
#[derive(Debug, Clone)]
pub struct Group {
    /// The container's label, such as `listitem #3`.
    pub label: String,
    /// The card's visible text, in reading order, without repeats.
    pub fields: Vec<String>,
    /// The control that opens or selects the card, when it has one.
    pub primary: Option<Candidate>,
}

/// The repeated cards on `screen`, in reading order; empty when nothing
/// repeats. Where several lists repeat, the one with the most cards.
#[must_use]
pub fn result_groups(screen: &Screen) -> Vec<Group> {
    result_families(screen)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Every list of repeated cards on `screen`, the longest first (the deeper
/// on a tie): a results page often repeats more than one thing, such as a
/// strip of dates above the flights themselves. Where no container repeats
/// by ordinal, the runs of same-role leaf siblings (`MIN_FLAT_ITEMS` or
/// more), each element one card.
#[must_use]
pub fn result_families(screen: &Screen) -> Vec<Vec<Group>> {
    let nodes = ordered(screen);
    let families: Vec<Vec<Group>> = list_levels(&nodes)
        .into_iter()
        .map(|(depth, parent)| cards(&nodes, depth, &parent, true))
        .filter(|groups| !groups.is_empty())
        .collect();
    if families.is_empty() {
        flat_lists(&nodes)
    } else {
        families
    }
}

/// A run of leaf siblings: their parent's path and their role.
type RunKey<'a> = (&'a [String], &'a str);

/// The runs of same-role leaf siblings on a screen that labels no container
/// by ordinal, the longest first (the deeper on a tie), each element with
/// text one card labelled by its role and place in the run.
fn flat_lists(nodes: &[(&Candidate, bool)]) -> Vec<Vec<Group>> {
    let mut runs: Vec<(RunKey<'_>, Vec<Group>)> = Vec::new();
    for (node, actionable) in nodes {
        if !node.children.is_empty() {
            continue;
        }
        let Some(text) = text_of(node, *actionable, true) else {
            continue;
        };
        let key = (node.path.as_slice(), node.role.as_str());
        let index = if let Some(index) = runs.iter().position(|(seen, _)| *seen == key) {
            index
        } else {
            runs.push((key, Vec::new()));
            runs.len() - 1
        };
        let groups = &mut runs[index].1;
        groups.push(Group {
            label: format!("{} #{}", node.role, groups.len() + 1),
            fields: vec![text],
            primary: actionable.then(|| (*node).clone()),
        });
    }
    runs.retain(|(_, groups)| groups.len() >= MIN_FLAT_ITEMS);
    runs.sort_by_key(|((path, _), groups)| std::cmp::Reverse((groups.len(), path.len())));
    runs.into_iter().map(|(_, groups)| groups).collect()
}

/// The cards of the list under `parent` whose containers sit at `depth`,
/// in reading order. `include_values` gates the same field content
/// [`super::screen::element_line`] gates: an unnamed field's held text, and
/// the ref-less text nodes that carry a rich-text area's body or a token
/// field's attachments, are included only when it is set.
pub(super) fn cards_at(
    screen: &Screen,
    depth: usize,
    parent: &[String],
    include_values: bool,
) -> Vec<Group> {
    cards(&ordered(screen), depth, parent, include_values)
}

/// Every node on `screen`, actionable or not, in document order.
fn ordered(screen: &Screen) -> Vec<(&Candidate, bool)> {
    let mut nodes = screen
        .candidates
        .iter()
        .map(|node| (node, true))
        .chain(screen.text_nodes.iter().map(|node| (node, false)))
        .collect::<Vec<_>>();
    nodes.sort_by_key(|(node, _)| node.order);
    nodes
}

/// The cards under `parent`, one per ordinal container at `depth`.
fn cards(
    nodes: &[(&Candidate, bool)],
    depth: usize,
    parent: &[String],
    include_values: bool,
) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (node, actionable) in nodes {
        if node.path.len() <= depth || node.path[..depth] != parent[..] {
            continue;
        }
        let container = &node.path[depth];
        if ordinal(container).is_none() {
            continue;
        }
        let index = if let Some(index) = groups.iter().position(|group| &group.label == container) {
            index
        } else {
            groups.push(Group {
                label: container.clone(),
                fields: Vec::new(),
                primary: None,
            });
            groups.len() - 1
        };
        let group = &mut groups[index];
        if let Some(text) =
            text_of(node, *actionable, include_values).filter(|text| !group.fields.contains(text))
        {
            group.fields.push(text);
        }
        if *actionable && prefers(node, group.primary.as_ref()) {
            group.primary = Some((*node).clone());
        }
    }
    groups.retain(|group| !group.fields.is_empty());
    groups
}

/// Every depth and parent path under which two or more same-role ordinal
/// containers repeat: the most containers first, the deeper on a tie.
fn list_levels(nodes: &[(&Candidate, bool)]) -> Vec<(usize, Vec<String>)> {
    let mut children: BTreeMap<(usize, Vec<String>, String), Vec<&String>> = BTreeMap::new();
    for (node, _) in nodes {
        for (depth, label) in node.path.iter().enumerate() {
            let Some((role, _)) = ordinal(label) else {
                continue;
            };
            let seen = children
                .entry((depth, node.path[..depth].to_vec(), role.to_owned()))
                .or_default();
            if !seen.contains(&label) {
                seen.push(label);
            }
        }
    }
    let mut levels = children
        .into_iter()
        .filter(|(_, labels)| labels.len() >= 2)
        .map(|((depth, parent, _), labels)| (labels.len(), depth, parent))
        .collect::<Vec<_>>();
    levels.sort_by_key(|(count, depth, _)| std::cmp::Reverse((*count, *depth)));
    levels
        .into_iter()
        .map(|(_, depth, parent)| (depth, parent))
        .collect()
}

/// `("listitem", 3)` for `listitem #3`, or for `listitem "Name" #3`.
pub(super) fn ordinal(label: &str) -> Option<(&str, usize)> {
    let (head, number) = label.rsplit_once(" #")?;
    let number = number.parse().ok()?;
    let role = head.split(' ').next().unwrap_or(head);
    Some((role, number))
}

/// Ancestor role prefixes — anywhere but the page or window root itself,
/// which names nothing about what it contains — under which a ref-less
/// node's value mirrors a field's held contents rather than ordinary card
/// text: a rich-text area's body (`webarea`, `document`) or a tokenized
/// field's own chip labels (`textbox`, `searchbox`, `combobox`, `textarea`).
/// Mirrors the ancestor check `tinycomputer_desktop`'s
/// `remembers_as_field_content` and `tinycomputer_browser`'s `FIELD_ROLES`
/// use to keep this same text out of `Screen::context` unconditionally.
const FIELD_ANCESTORS: &[&str] = &[
    "webarea",
    "document",
    "textbox",
    "searchbox",
    "combobox",
    "textarea",
];

/// Whether `path` sits inside one of [`FIELD_ANCESTORS`] below the root: a
/// ref-less node there holds field content, private unless values are
/// shared, even though it carries no ref of its own. The root itself (index
/// 0) is skipped — a whole page hosted in one `webarea`, as a browser tab
/// is, must not make every card on it field content.
fn inside_field_content(path: &[String]) -> bool {
    path.iter().skip(1).any(|ancestor| {
        let ancestor = ancestor.to_ascii_lowercase();
        FIELD_ANCESTORS
            .iter()
            .any(|role| ancestor.starts_with(role))
    })
}

/// A card field's visible text: an actionable node's name or description
/// unconditionally, falling back to its held value only when `include_values`
/// is set — the same gate [`super::screen::element_line`] applies to an
/// ordinary element. A ref-less text node's name or description is shown
/// unconditionally too (most of it is ordinary card text — a price, an
/// airline name — that never reaches `context` only because it carries no
/// ref of its own); its value is gated the same way unless it sits inside a
/// rich-text area or a tokenized field ([`inside_field_content`]), where it
/// mirrors that field's held contents rather than naming anything of its
/// own.
fn text_of(node: &Candidate, actionable: bool, include_values: bool) -> Option<String> {
    let named = || node.name.clone().or_else(|| node.description.clone());
    let held_value = || match &node.value {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    };
    let gate_value = actionable || inside_field_content(&node.path);
    let text = named().or_else(|| {
        if gate_value {
            include_values.then(held_value).flatten()
        } else {
            held_value()
        }
    })?;
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

/// Whether `node` is a better "open this card" control than `current`.
fn prefers(node: &Candidate, current: Option<&Candidate>) -> bool {
    let opens = |candidate: &Candidate| {
        let name = candidate.name.as_deref().unwrap_or_default().to_lowercase();
        OPENERS.iter().any(|word| name.contains(word))
    };
    match current {
        None => true,
        Some(current) => opens(node) && !opens(current),
    }
}
