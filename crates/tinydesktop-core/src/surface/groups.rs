//! Repeated cards on a screen — search results, listings, inboxes — as
//! records a task can rank.
//!
//! A surface labels repeated containers with an ordinal (`listitem #3`), so
//! every node under one card shares that card's label in its path. The list
//! is the parent under which the most same-role containers repeat; each
//! container is one record, its text is the record's fields, and its most
//! "open this" control is how the record is chosen.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{Candidate, Screen};

/// Words that mark the control that opens or selects a card.
const OPENERS: &[&str] = &[
    "select", "book", "choose", "view", "details", "continue", "reserve", "deal", "see",
];

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
/// repeats.
#[must_use]
pub fn result_groups(screen: &Screen) -> Vec<Group> {
    let mut nodes = screen
        .candidates
        .iter()
        .map(|node| (node, true))
        .chain(screen.text_nodes.iter().map(|node| (node, false)))
        .collect::<Vec<_>>();
    nodes.sort_by_key(|(node, _)| node.order);

    let Some((depth, parent)) = list_level(&nodes) else {
        return Vec::new();
    };
    let mut groups: Vec<Group> = Vec::new();
    for (node, actionable) in nodes {
        if node.path.len() <= depth || node.path[..depth] != parent[..] {
            continue;
        }
        let container = &node.path[depth];
        if ordinal(container).is_none() {
            continue;
        }
        let index = match groups.iter().position(|group| &group.label == container) {
            Some(index) => index,
            None => {
                groups.push(Group {
                    label: container.clone(),
                    fields: Vec::new(),
                    primary: None,
                });
                groups.len() - 1
            }
        };
        let group = &mut groups[index];
        if let Some(text) = text_of(node).filter(|text| !group.fields.contains(text)) {
            group.fields.push(text);
        }
        if actionable && prefers(node, group.primary.as_ref()) {
            group.primary = Some(node.clone());
        }
    }
    groups.retain(|group| !group.fields.is_empty());
    groups
}

/// The depth and parent path under which the most same-role ordinal
/// containers repeat; the deeper wins a tie.
fn list_level(nodes: &[(&Candidate, bool)]) -> Option<(usize, Vec<String>)> {
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
    children
        .into_iter()
        .filter(|(_, labels)| labels.len() >= 2)
        .max_by_key(|((depth, _, _), labels)| (labels.len(), *depth))
        .map(|((depth, parent, _), _)| (depth, parent))
}

/// `("listitem", 3)` for `listitem #3`, or for `listitem "Name" #3`.
fn ordinal(label: &str) -> Option<(&str, usize)> {
    let (head, number) = label.rsplit_once(" #")?;
    let number = number.parse().ok()?;
    let role = head.split(' ').next().unwrap_or(head);
    Some((role, number))
}

fn text_of(node: &Candidate) -> Option<String> {
    let text = node
        .name
        .clone()
        .or_else(|| node.description.clone())
        .or_else(|| match &node.value {
            Some(Value::String(value)) => Some(value.clone()),
            _ => None,
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
