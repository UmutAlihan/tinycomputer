//! What a surface shows: candidates, context, and the judgements made about
//! them without asking a model — fingerprints, change notes, labels.

use std::fmt::Write as _;

use serde::Deserialize;
use serde_json::{Value, json};
use tinycomputer_bus::JevTarget;

/// Most actionable candidates one observation offers.
pub const MAX_CANDIDATES: usize = 254;

/// Changed labels listed per change note.
const HISTORY_CHANGES: usize = 6;

/// One ref-bearing accessibility node offered to Jev.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Candidate {
    /// The engine ref that addresses this node in its snapshot.
    pub ref_id: String,
    /// The accessibility role, such as `button` or `textfield`.
    pub role: String,
    /// The accessible name.
    pub name: Option<String>,
    /// The accessible description, used when there is no name.
    pub description: Option<String>,
    /// The current value, for fields and controls that hold one.
    pub value: Option<Value>,
    /// States such as `focused`, `selected`, or `disabled`.
    pub states: Vec<String>,
    /// The actions the engine can perform on it, such as `Click`.
    pub available_actions: Vec<String>,
    /// How many children it has, when the engine reports it.
    pub children_count: Option<usize>,
    /// Its on-screen rectangle, when known.
    pub bounds: Option<Value>,
    /// Its child nodes, as the snapshot nested them.
    pub children: Vec<Candidate>,
    /// Whether the engine cut this node's subtree short to stay in budget.
    pub subtree_truncated: bool,
    #[serde(skip)]
    /// Labels of its ancestors, outermost first.
    pub path: Vec<String>,
    /// This node's position in the tree's document order, so a ref-bearing
    /// and a ref-less node can be merged back into reading order even though
    /// they are collected into separate lists.
    #[serde(skip)]
    pub order: usize,
}

/// Parsed current surface.
#[derive(Debug, Clone)]
pub struct Screen {
    /// The application (or site) the screen belongs to.
    pub app: String,
    /// The window or page title.
    pub window: Option<String>,
    /// What is in front: `window`, or an overlay such as `sheet`.
    pub surface: String,
    /// Ref-bearing, actionable nodes, in document order.
    pub candidates: Vec<Candidate>,
    /// Visible non-actionable text (labels, headings, status), in tree order.
    pub context: Vec<String>,
    /// Refs of subtrees the engine cut short; observing one as a root reads
    /// what the budget left out.
    pub unexplored: Vec<String>,
    /// Ref-less nodes not already summarized in `context`: the static text
    /// carrying a rich-text area's body or a token field's attachments. Kept
    /// separately, and in document order via [`Candidate::order`], so field
    /// content stays reachable to the flow runtime's
    /// `field_contents` — gated on `include_values` — without ever reaching
    /// `context`, which every request shares unconditionally.
    pub text_nodes: Vec<Candidate>,
}

/// How much of the tree one observation reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Depth {
    /// The engine's default depth.
    #[default]
    Full,
    /// A shallow overview whose cut-off containers can be drilled into.
    Skeleton,
}

/// Describes a node for Jev, wrapped so it is treated as untrusted data;
/// field values are included only when `include_values` is set.
#[must_use]
pub fn describe(node: &Candidate, include_values: bool) -> Value {
    let mut value = json!({
        "what": format!(
            "{}{}",
            node.role,
            node.name
                .as_deref()
                .or(node.description.as_deref())
                .map_or_else(String::new, |name| format!(" {name:?}"))
        ),
        "where": if node.path.is_empty() { "top level".to_owned() } else { node.path.join(" > ") },
        "supports": node.available_actions,
    });
    if include_values && let Some(held) = node.value.as_ref().filter(|value| !value.is_null()) {
        value["holds"] = Value::String(held.to_string().chars().take(120).collect());
    }
    if !node.states.is_empty() {
        value["state"] = Value::String(node.states.join(", "));
    }
    if let Some(count) = node.children_count {
        value["contains"] = json!(count);
    }
    if let Some(near) = near(node) {
        value["near"] = Value::String(near);
    }
    if node.name.is_none()
        && node.description.is_none()
        && node.value.is_none()
        && let Some(bounds) = &node.bounds
    {
        value["bounds"] = bounds.clone();
    }
    json!({"untrusted_accessibility_data": value})
}

/// Identifies what is on screen independently of ref allocation.
///
/// Refs are re-minted by every snapshot, so a fingerprint that included them
/// would report a change on every turn and stall detection would never fire.
#[must_use]
pub fn fingerprint(screen: &Screen) -> String {
    let mut parts = screen.candidates.iter().map(signature).collect::<Vec<_>>();
    parts.push(format!(
        "window:{}",
        screen.window.as_deref().unwrap_or_default()
    ));
    parts.push(format!("surface:{}", screen.surface));
    parts.extend(screen.context.iter().map(|line| format!("text:{line}")));
    parts.join("|")
}

/// A ref-free identity for one element: role, label, value, states, and where
/// it sits.
#[must_use]
pub fn signature(node: &Candidate) -> String {
    format!(
        "{}:{}:{}:{:?}:{}",
        node.role,
        node.name
            .as_deref()
            .or(node.description.as_deref())
            .unwrap_or_default(),
        node.value
            .as_ref()
            .map(Value::to_string)
            .unwrap_or_default(),
        node.states,
        node.path.join(">")
    )
}

/// Longest `near` label shown for an unnamed element, in characters.
const NEAR_CHARS: usize = 60;

/// For an element with no name of its own, the nearest named container it
/// sits in, clipped: an unnamed search box inside `button "destinationCity"`
/// is told apart from the unnamed boxes of every other dropdown only by it.
/// `None` for a named element, or one with no named container.
fn near(node: &Candidate) -> Option<String> {
    if node.name.is_some() || node.description.is_some() {
        return None;
    }
    let container = node.path.iter().rev().find(|label| label.contains('"'))?;
    let mut clipped = container.chars().take(NEAR_CHARS).collect::<String>();
    if container.chars().count() > NEAR_CHARS {
        clipped.push('…');
    }
    Some(clipped)
}

/// One element as a line of the state Jev reads: its label, where an
/// unnamed element sits, what it holds when `include_values` is set (up to
/// 80 characters), and its states.
#[must_use]
pub fn element_line(node: &Candidate, include_values: bool) -> String {
    let mut line = label(node);
    if let Some(near) = near(node) {
        let _ = write!(line, " in {near}");
    }
    if include_values
        && let Some(value) = node.value.as_ref().and_then(Value::as_str)
        && !value.is_empty()
    {
        let shown: String = value.chars().take(80).collect();
        let _ = write!(line, " = {shown:?}");
    }
    if !node.states.is_empty() {
        let _ = write!(line, " [{}]", node.states.join(", "));
    }
    line
}

/// A short human label for an element: role plus accessible name.
#[must_use]
pub fn label(node: &Candidate) -> String {
    node.name
        .as_deref()
        .or(node.description.as_deref())
        .map_or_else(
            || node.role.clone(),
            |name| format!("{} {name:?}", node.role),
        )
}

/// Element labels present in `after` but not `before`, and the reverse.
#[must_use]
pub fn difference(before: &Screen, after: &Screen) -> (Vec<String>, Vec<String>) {
    let labels = |screen: &Screen| {
        screen
            .candidates
            .iter()
            .map(label)
            .chain(screen.context.iter().cloned())
            .collect::<std::collections::BTreeSet<_>>()
    };
    let (before, after) = (labels(before), labels(after));
    (
        after.difference(&before).cloned().collect(),
        before.difference(&after).cloned().collect(),
    )
}

/// Describes what an action changed, in labels Jev can match next turn.
#[must_use]
pub fn change_note(before: &Screen, after: &Screen, changed: bool) -> String {
    if !changed {
        return "nothing on screen changed".to_owned();
    }
    let (appeared, disappeared) = difference(before, after);
    let mut parts = Vec::new();
    if before.window != after.window {
        parts.push(format!(
            "window is now {:?}",
            after.window.as_deref().unwrap_or_default()
        ));
    }
    if before.surface != after.surface {
        parts.push(format!("surface is now {}", after.surface));
    }
    if !appeared.is_empty() {
        parts.push(format!("appeared: {}", summarize(&appeared)));
    }
    if !disappeared.is_empty() {
        parts.push(format!("gone: {}", summarize(&disappeared)));
    }
    if parts.is_empty() {
        "the screen changed".to_owned()
    } else {
        parts.join("; ")
    }
}

fn summarize(labels: &[String]) -> String {
    let mut shown = labels
        .iter()
        .take(HISTORY_CHANGES)
        .cloned()
        .collect::<Vec<_>>();
    if labels.len() > HISTORY_CHANGES {
        shown.push(format!("and {} more", labels.len() - HISTORY_CHANGES));
    }
    shown.join(", ")
}

/// The screen's static text, wrapped so Jev treats it as data.
#[must_use]
pub fn untrusted_context(screen: &Screen) -> serde_json::Value {
    json!({"untrusted_accessibility_data": screen.context})
}

/// Whether `goal` names the candidate exactly: a run of at least two of its
/// name's words, in order, appears in the goal.
#[must_use]
pub fn exact_named_match(goal: &str, candidate: Option<&Candidate>) -> bool {
    let Some(name) = candidate.and_then(|candidate| {
        candidate
            .name
            .as_deref()
            .or(candidate.description.as_deref())
    }) else {
        return false;
    };
    let normalize = |value: &str| {
        let normalized = value
            .chars()
            .map(|character| {
                if character.is_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    ' '
                }
            })
            .collect::<String>();
        normalized.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let name = normalize(name);
    let goal = normalize(goal);
    let words = name.split_whitespace().collect::<Vec<_>>();
    if words.len() < 2 {
        return false;
    }
    let padded_goal = format!(" {goal} ");
    (2..=words.len()).rev().any(|length| {
        let prefix = words[..length].join(" ");
        padded_goal.contains(&format!(" {prefix} "))
    })
}

/// The wire form of an element a flow acted on.
#[must_use]
pub fn target_payload(candidate: &Candidate) -> JevTarget {
    JevTarget {
        ref_id: candidate.ref_id.clone(),
        role: candidate.role.clone(),
        name: candidate
            .name
            .clone()
            .or_else(|| candidate.description.clone()),
    }
}
