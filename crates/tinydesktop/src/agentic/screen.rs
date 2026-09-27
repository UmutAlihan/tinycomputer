//! Accessibility snapshot parsing and compact Jev candidate descriptions.

use serde::Deserialize;
use serde_json::{Value, json};
use tinydesktop_bus::{DesktopResponse, ListWindowsRequest, SnapshotRequest, Surface};

use crate::Desktop;

const MAX_TREE_DEPTH: usize = 64;
const MAX_VISITED_NODES: usize = 4_096;
/// Most actionable candidates one observation offers.
pub(super) const MAX_CANDIDATES: usize = 254;
/// Most static-text lines one observation keeps as context.
const MAX_CONTEXT_LINES: usize = 60;
/// Longest static-text line kept as context, in characters.
const MAX_CONTEXT_CHARS: usize = 160;

/// One ref-bearing accessibility node offered to Jev.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub(super) struct Candidate {
    pub(super) ref_id: String,
    pub(super) role: String,
    pub(super) name: Option<String>,
    pub(super) description: Option<String>,
    pub(super) value: Option<Value>,
    pub(super) states: Vec<String>,
    pub(super) available_actions: Vec<String>,
    pub(super) children_count: Option<usize>,
    pub(super) bounds: Option<Value>,
    pub(super) children: Vec<Candidate>,
    /// Whether the engine cut this node's subtree short to stay in budget.
    pub(super) subtree_truncated: bool,
    #[serde(skip)]
    pub(super) path: Vec<String>,
}

/// Parsed current surface.
#[derive(Debug, Clone)]
pub(super) struct Screen {
    pub(super) app: String,
    pub(super) window: Option<String>,
    pub(super) surface: String,
    pub(super) root: Option<String>,
    pub(super) candidates: Vec<Candidate>,
    /// Visible non-actionable text (labels, headings, status), in tree order.
    pub(super) context: Vec<String>,
    /// `(kept, total)` when more candidates existed than were offered.
    pub(super) truncated: Option<(usize, usize)>,
    /// Refs of subtrees the engine cut short; observing one as a root reads
    /// what the budget left out.
    pub(super) unexplored: Vec<String>,
}

/// How much of the tree one observation reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Depth {
    /// The engine's default depth.
    #[default]
    Full,
    /// A shallow overview whose cut-off containers can be drilled into.
    Skeleton,
}

pub(super) fn observe(
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
        && reply
            .error
            .as_ref()
            .is_some_and(|error| error.code == "AMBIGUOUS_TARGET")
        && let Some(window_id) = front_window(desktop, app)
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

/// The window to observe when an application has several: its focused one,
/// else its first visible one.
fn front_window(desktop: &Desktop, app: &str) -> Option<String> {
    let reply = desktop.list_windows(ListWindowsRequest {
        app: Some(app.to_owned()),
    });
    let windows = reply.data?.as_array()?.clone();
    let flag = |window: &Value, key: &str| window.get(key).and_then(Value::as_bool) == Some(true);
    windows
        .iter()
        .find(|window| flag(window, "is_focused"))
        .or_else(|| windows.iter().find(|window| flag(window, "visible")))
        .and_then(|window| window.get("id").and_then(Value::as_str))
        .map(str::to_owned)
}

pub(super) fn parse_reply(
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
    let mut visited = 0_usize;
    collect(
        &mut root_node,
        &[],
        &mut Collected {
            candidates: &mut candidates,
            context: &mut context,
            unexplored: &mut unexplored,
        },
        0,
        &mut visited,
    );
    candidates.retain(offerable);
    let total = candidates.len();
    candidates.truncate(MAX_CANDIDATES);
    let truncated = (total > MAX_CANDIDATES).then_some((MAX_CANDIDATES, total));

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
        root: root.map(str::to_owned),
        candidates,
        context,
        truncated,
        unexplored,
    })
}

struct Collected<'a> {
    candidates: &'a mut Vec<Candidate>,
    context: &'a mut Vec<String>,
    unexplored: &'a mut Vec<String>,
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
        remember_text(node, out.context);
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

pub(super) fn describe(node: &Candidate, include_values: bool) -> Value {
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
pub(super) fn fingerprint(screen: &Screen) -> String {
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
pub(super) fn signature(node: &Candidate) -> String {
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
pub(super) fn label(node: &Candidate) -> String {
    node.name
        .as_deref()
        .or(node.description.as_deref())
        .map_or_else(
            || node.role.clone(),
            |name| format!("{} {name:?}", node.role),
        )
}

/// Element labels present in `after` but not `before`, and the reverse.
pub(super) fn difference(before: &Screen, after: &Screen) -> (Vec<String>, Vec<String>) {
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
