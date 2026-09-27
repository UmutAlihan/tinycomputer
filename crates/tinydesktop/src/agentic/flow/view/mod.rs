//! What a flow sees: bounded snapshot parsing, candidates, context, and the
//! small judgements made about them without asking Jev.
//!
//! The flow runtime keeps its own observation rather than sharing `RunGoal`'s:
//! it needs static text as context, truncation markers, the subtrees the
//! engine cut short, and window disambiguation, none of which the scoped
//! goal task uses.

use serde::Deserialize;
use serde_json::{Value, json};
use tinydesktop_bus::{DesktopResponse, JevTarget, ListWindowsRequest, SnapshotRequest, Surface};

use crate::Desktop;

/// Least probability a target choice needs to be used without re-asking.
pub(in crate::agentic) const ACT: f64 = 0.70;

const MAX_TREE_DEPTH: usize = 64;
const MAX_VISITED_NODES: usize = 4_096;
/// Most actionable candidates one observation offers.
pub(in crate::agentic) const MAX_CANDIDATES: usize = 254;
/// Most static-text lines one observation keeps as context.
const MAX_CONTEXT_LINES: usize = 60;
/// Longest static-text line kept as context, in characters.
const MAX_CONTEXT_CHARS: usize = 160;
/// Changed labels listed per change note.
const HISTORY_CHANGES: usize = 6;

/// One ref-bearing accessibility node offered to Jev.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub(in crate::agentic) struct Candidate {
    pub(in crate::agentic) ref_id: String,
    pub(in crate::agentic) role: String,
    pub(in crate::agentic) name: Option<String>,
    pub(in crate::agentic) description: Option<String>,
    pub(in crate::agentic) value: Option<Value>,
    pub(in crate::agentic) states: Vec<String>,
    pub(in crate::agentic) available_actions: Vec<String>,
    pub(in crate::agentic) children_count: Option<usize>,
    pub(in crate::agentic) bounds: Option<Value>,
    pub(in crate::agentic) children: Vec<Candidate>,
    /// Whether the engine cut this node's subtree short to stay in budget.
    pub(in crate::agentic) subtree_truncated: bool,
    #[serde(skip)]
    pub(in crate::agentic) path: Vec<String>,
    /// This node's position in the tree's document order, so a ref-bearing
    /// and a ref-less node can be merged back into reading order even though
    /// they are collected into separate lists.
    #[serde(skip)]
    pub(in crate::agentic) order: usize,
}

/// Parsed current surface.
#[derive(Debug, Clone)]
pub(in crate::agentic) struct Screen {
    pub(in crate::agentic) app: String,
    pub(in crate::agentic) window: Option<String>,
    pub(in crate::agentic) surface: String,
    pub(in crate::agentic) candidates: Vec<Candidate>,
    /// Visible non-actionable text (labels, headings, status), in tree order.
    pub(in crate::agentic) context: Vec<String>,
    /// Refs of subtrees the engine cut short; observing one as a root reads
    /// what the budget left out.
    pub(in crate::agentic) unexplored: Vec<String>,
    /// Ref-less nodes not already summarized in `context`: the static text
    /// carrying a rich-text area's body or a token field's attachments. Kept
    /// separately, and in document order via [`Candidate::order`], so field
    /// content stays reachable to [`crate::agentic::flow::ask`]'s
    /// `field_contents` — gated on `include_values` — without ever reaching
    /// `context`, which every request shares unconditionally.
    pub(in crate::agentic) text_nodes: Vec<Candidate>,
}

/// How much of the tree one observation reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::agentic) enum Depth {
    /// The engine's default depth.
    #[default]
    Full,
    /// A shallow overview whose cut-off containers can be drilled into.
    Skeleton,
}

pub(in crate::agentic) fn observe(
    desktop: &Desktop,
    app: &str,
    root: Option<&str>,
    depth: Depth,
) -> Result<Screen, Box<DesktopResponse>> {
    let request = SnapshotRequest {
        app: Some(app.to_owned()),
        include_bounds: true,
        compact: true,
        skeleton: depth == Depth::Skeleton && root.is_none(),
        root_ref: root.map(str::to_owned),
        ..SnapshotRequest::default()
    };
    let mut reply = desktop.snapshot(request.clone());
    if root.is_none()
        && let Some(error) = reply
            .error
            .as_ref()
            .filter(|error| error.code == "AMBIGUOUS_TARGET")
        && let Some(window_id) = error
            .details
            .as_ref()
            .and_then(|details| details.get("candidates"))
            .and_then(|candidates| front_of(candidates.as_array()?))
            .or_else(|| front_window(desktop, app))
    {
        reply = desktop.snapshot(SnapshotRequest {
            window_id: Some(window_id),
            ..request
        });
    }
    if !reply.ok && root.is_none() {
        reply = desktop.snapshot(SnapshotRequest {
            app: Some(app.to_owned()),
            max_depth: Some(4),
            include_bounds: true,
            compact: true,
            ..SnapshotRequest::default()
        });
    }
    parse_reply(desktop, app, root, reply)
}

/// The window to observe when an application has several, from its own
/// window list.
fn front_window(desktop: &Desktop, app: &str) -> Option<String> {
    let reply = desktop.list_windows(ListWindowsRequest {
        app: Some(app.to_owned()),
    });
    front_of(reply.data?.as_array()?)
}

/// The focused window, else the first visible one with a title, else the
/// first visible one.
pub(in crate::agentic) fn front_of(windows: &[Value]) -> Option<String> {
    let flag = |window: &Value, key: &str| window.get(key).and_then(Value::as_bool) == Some(true);
    let titled = |window: &&Value| {
        window
            .get("title")
            .and_then(Value::as_str)
            .is_some_and(|title| !title.trim().is_empty())
    };
    windows
        .iter()
        .find(|window| flag(window, "is_focused"))
        .or_else(|| {
            windows
                .iter()
                .filter(|window| flag(window, "visible"))
                .find(titled)
        })
        .or_else(|| windows.iter().find(|window| flag(window, "visible")))
        .and_then(|window| window.get("id").and_then(Value::as_str))
        .map(str::to_owned)
}

pub(in crate::agentic) fn parse_reply(
    desktop: &Desktop,
    app: &str,
    root: Option<&str>,
    mut reply: DesktopResponse,
) -> Result<Screen, Box<DesktopResponse>> {
    if !reply.ok {
        return Err(Box::new(reply));
    }
    let mut surface = "window".to_owned();
    if root.is_none()
        && let Some(role) = overlay_role(reply.data.as_ref().and_then(|data| data.get("tree")))
    {
        let overlay = match role {
            "sheet" => Surface::Sheet,
            "alert" => Surface::Alert,
            "menu" => Surface::Menu,
            "popover" => Surface::Popover,
            _ => Surface::Window,
        };
        let scoped = desktop.snapshot(SnapshotRequest {
            app: Some(app.to_owned()),
            include_bounds: true,
            compact: true,
            surface: overlay,
            ..SnapshotRequest::default()
        });
        if scoped.ok {
            reply = scoped;
            role.clone_into(&mut surface);
        }
    }
    let Some(data) = reply.data.as_ref() else {
        return Err(Box::new(DesktopResponse::err(
            "snapshot",
            tinydesktop_bus::DesktopError::new("INTERNAL", "successful snapshot carried no data"),
        )));
    };
    let tree = data.get("tree").cloned().unwrap_or(Value::Null);
    let mut root_node: Candidate = serde_json::from_value(tree).unwrap_or_default();
    let mut candidates = Vec::new();
    let mut context = Vec::new();
    let mut unexplored = Vec::new();
    let mut text_nodes = Vec::new();
    let mut visited = 0_usize;
    collect(
        &mut root_node,
        &[],
        &mut Collected {
            candidates: &mut candidates,
            context: &mut context,
            unexplored: &mut unexplored,
            text_nodes: &mut text_nodes,
        },
        0,
        &mut visited,
    );
    // Not truncated here: a flow narrows a large pool region by region
    // instead of cutting it.
    candidates.retain(offerable);

    Ok(Screen {
        app: data
            .get("app")
            .and_then(Value::as_str)
            .unwrap_or(app)
            .to_owned(),
        window: data
            .get("window")
            .and_then(|window| window.get("title"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        surface,
        candidates,
        context,
        unexplored,
        text_nodes,
    })
}

struct Collected<'a> {
    candidates: &'a mut Vec<Candidate>,
    unexplored: &'a mut Vec<String>,
    text_nodes: &'a mut Vec<Candidate>,
}

fn collect(
    node: &mut Candidate,
    path: &[String],
    out: &mut Collected<'_>,
    depth: usize,
    visited: &mut usize,
) {
    if depth > MAX_TREE_DEPTH || *visited >= MAX_VISITED_NODES {
        return;
    }
    node.order = *visited;
    *visited = visited.saturating_add(1);
    let label = node
        .name
        .as_deref()
        .or(node.description.as_deref())
        .map_or_else(
            || node.role.clone(),
            |name| format!("{} {name:?}", node.role),
        );
    node.path = path.to_vec();
    if node.subtree_truncated && !node.ref_id.is_empty() {
        out.unexplored.push(node.ref_id.clone());
    }
    if node.ref_id.is_empty() {
        if !remembers_as_field_content(node) {
            remember_text(node, out.context);
        }
        out.text_nodes.push(node.clone());
    } else {
        out.candidates.push(node.clone());
    }
    let mut child_path = path.to_vec();
    if !node.children.is_empty() {
        child_path.push(label);
    }
    for child in &mut node.children {
        collect(child, &child_path, out, depth.saturating_add(1), visited);
    }
}

/// Whether a ref-less node's text is a mirror of a field's contents rather
/// than screen chrome: any static text nested inside a rich-text area, where
/// a mail body or a web view spreads its held text over such descendants
/// because the area itself carries none.
///
/// Such a node must never reach `context`, which every request shares
/// regardless of `include_values` — it is field content, so it is left for
/// [`crate::agentic::flow::ask`]'s gated `field_contents` to read from
/// [`Screen::text_nodes`] instead.
fn remembers_as_field_content(node: &Candidate) -> bool {
    node.path.iter().any(|ancestor| {
        let ancestor = ancestor.to_ascii_lowercase();
        ancestor.starts_with("webarea") || ancestor.starts_with("document")
    })
}

/// Keeps a ref-less node's visible text as context for Jev.
///
/// Labels, headings, and status text are what tell a decision model where it
/// is ("New Message", "Now playing"), and none of them carry a ref because none
/// of them can be acted on.
fn remember_text(node: &Candidate, context: &mut Vec<String>) {
    if context.len() >= MAX_CONTEXT_LINES {
        return;
    }
    let text = node
        .name
        .as_deref()
        .or(node.description.as_deref())
        .map(str::to_owned)
        .or_else(|| match node.value.as_ref() {
            Some(Value::String(value)) => Some(value.clone()),
            _ => None,
        });
    let Some(text) = text.map(|text| text.split_whitespace().collect::<Vec<_>>().join(" ")) else {
        return;
    };
    if text.is_empty() {
        return;
    }
    let line: String = text.chars().take(MAX_CONTEXT_CHARS).collect();
    if !context.contains(&line) {
        context.push(line);
    }
}

fn offerable(node: &Candidate) -> bool {
    !node.available_actions.is_empty()
        && !node
            .states
            .iter()
            .any(|state| matches!(state.as_str(), "disabled" | "hidden"))
}

fn overlay_role(tree: Option<&Value>) -> Option<&'static str> {
    let mut pending = vec![(tree?, 0_usize)];
    let mut visited = 0_usize;
    while let Some((node, depth)) = pending.pop() {
        if depth > MAX_TREE_DEPTH || visited >= MAX_VISITED_NODES {
            continue;
        }
        visited = visited.saturating_add(1);
        if let Some(role) = node.get("role").and_then(Value::as_str)
            && matches!(role, "sheet" | "alert" | "menu" | "popover")
        {
            return Some(match role {
                "sheet" => "sheet",
                "alert" => "alert",
                "menu" => "menu",
                _ => "popover",
            });
        }
        if let Some(children) = node.get("children").and_then(Value::as_array) {
            pending.extend(
                children
                    .iter()
                    .rev()
                    .map(|child| (child, depth.saturating_add(1))),
            );
        }
    }
    None
}

pub(in crate::agentic) fn describe(node: &Candidate, include_values: bool) -> Value {
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
pub(in crate::agentic) fn fingerprint(screen: &Screen) -> String {
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
pub(in crate::agentic) fn signature(node: &Candidate) -> String {
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

/// A short human label for an element: role plus accessible name.
pub(in crate::agentic) fn label(node: &Candidate) -> String {
    node.name
        .as_deref()
        .or(node.description.as_deref())
        .map_or_else(
            || node.role.clone(),
            |name| format!("{} {name:?}", node.role),
        )
}

/// Element labels present in `after` but not `before`, and the reverse.
pub(in crate::agentic) fn difference(
    before: &Screen,
    after: &Screen,
) -> (Vec<String>, Vec<String>) {
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
pub(in crate::agentic) fn change_note(before: &Screen, after: &Screen, changed: bool) -> String {
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
pub(in crate::agentic) fn untrusted_context(screen: &Screen) -> serde_json::Value {
    json!({"untrusted_accessibility_data": screen.context})
}

pub(in crate::agentic) fn exact_named_match(goal: &str, candidate: Option<&Candidate>) -> bool {
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

/// Whether a lower-cased label names an action that is hard to undo.
pub(in crate::agentic) fn destructive_label(evidence: &str) -> bool {
    [
        "delete",
        "remove",
        "send",
        "purchase",
        "buy",
        "pay",
        "submit",
        "confirm",
        "overwrite",
        "quit without saving",
        "empty trash",
        "sign out",
    ]
    .iter()
    .any(|term| evidence.contains(term))
}

/// Whether `label` is named by a `stop_before` phrase the flow itself
/// declares elsewhere.
///
/// A flow that already plans to `stop_before: "sending the email"` has told
/// us, in its own words, that whatever performs that action is irreversible —
/// even when the generic English denylist above does not happen to cover the
/// word it uses. A label under three characters is never checked: it is too
/// short for containment to mean anything ("ok", "go") and would otherwise
/// match almost any phrase.
pub(in crate::agentic) fn named_in_stop_before(label: &str, stop_before: &[String]) -> bool {
    let label = label.trim().to_ascii_lowercase();
    label.chars().count() >= 3
        && stop_before
            .iter()
            .any(|phrase| phrase.to_ascii_lowercase().contains(&label))
}

/// Whether pressing `candidate` on `screen` must be treated as irreversible:
/// its own label names a hard-to-undo action, the flow's own `stop_before`
/// steps already name it, or it is an unnamed control offered inside a
/// confirmation sheet — the shape of "Delete"/"Cancel" dialogs whose default
/// button carries no accessible name on some platforms, so the denylist can
/// never see the word that would otherwise gate it.
pub(in crate::agentic) fn is_destructive(
    candidate: &Candidate,
    screen: &Screen,
    stop_before: &[String],
) -> bool {
    let name = candidate
        .name
        .as_deref()
        .or(candidate.description.as_deref())
        .unwrap_or_default();
    destructive_label(&label(candidate).to_ascii_lowercase())
        || named_in_stop_before(name, stop_before)
        || (screen.surface == "sheet" && candidate.name.is_none())
}

/// The wire form of an element a flow acted on.
pub(in crate::agentic) fn target_payload(candidate: &Candidate) -> JevTarget {
    JevTarget {
        ref_id: candidate.ref_id.clone(),
        role: candidate.role.clone(),
        name: candidate
            .name
            .clone()
            .or_else(|| candidate.description.clone()),
    }
}

#[cfg(test)]
mod test;
