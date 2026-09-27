//! [`Desktop`] as a [`Surface`]: agent-desktop snapshots parsed into what a
//! decision loop sees, and the closed operations it acts with.
//!
//! Observation keeps static text as context, records subtrees the engine cut
//! short, keeps field content out of that shared context, and resolves an
//! application with several windows to its front one. Paste goes through the
//! clipboard and puts back what it held.

use serde_json::{Value, json};
use tinydesktop_bus::{
    ClipboardFormat, ClipboardGetRequest, ClipboardSetRequest, DesktopResponse, Direction,
    ElementProperty, GetRequest, JevOperation, LaunchRequest, ListWindowsRequest, PressRequest,
    RefRequest, ScrollRequest, SetValueRequest, SnapshotRequest, Surface as Overlay, WaitRequest,
};
use tinydesktop_core::surface::{Candidate, Depth, Screen, Surface};

use crate::Desktop;

const MAX_TREE_DEPTH: usize = 64;

const MAX_VISITED_NODES: usize = 4_096;

/// Most static-text lines one observation keeps as context.
const MAX_CONTEXT_LINES: usize = 60;

/// Longest static-text line kept as context, in characters.
const MAX_CONTEXT_CHARS: usize = 160;

pub(crate) fn observe(
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
pub(crate) fn front_of(windows: &[Value]) -> Option<String> {
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

pub(crate) fn parse_reply(
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
            "sheet" => Overlay::Sheet,
            "alert" => Overlay::Alert,
            "menu" => Overlay::Menu,
            "popover" => Overlay::Popover,
            _ => Overlay::Window,
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
    let mut unexplored = Vec::new();
    let mut text_nodes = Vec::new();
    let mut visited = 0_usize;
    collect(
        &mut root_node,
        &[],
        &mut Collected {
            candidates: &mut candidates,
            unexplored: &mut unexplored,
            text_nodes: &mut text_nodes,
        },
        0,
        &mut visited,
    );
    let context = build_context(&candidates, &text_nodes);
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
        // Whether this text belongs in `context` is decided once the whole
        // tree — and every node's document order — is known, in
        // `build_context` below; a token field's chip labels are ordinary
        // siblings of the field, not descendants, so that decision cannot be
        // made node-by-node during this traversal.
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

/// Builds `Screen::context` from the ref-less nodes `collect` set aside,
/// leaving out whatever is field content rather than screen chrome.
///
/// This runs once the whole tree — and every node's [`Candidate::order`] — is
/// known, because a token field's chip labels are its ordinary siblings, not
/// its descendants: recognizing them needs the field that precedes them in
/// document order, which [`remembers_as_field_content`]'s ancestor-only check
/// cannot see during the traversal that builds `text_nodes` node by node.
fn build_context(candidates: &[Candidate], text_nodes: &[Candidate]) -> Vec<String> {
    let mut ordered = candidates
        .iter()
        .chain(text_nodes.iter())
        .collect::<Vec<_>>();
    ordered.sort_by_key(|node| node.order);
    let mut context = Vec::new();
    for node in text_nodes {
        if !remembers_as_field_content(node) && !follows_a_settable_field(&ordered, node) {
            remember_text(node, &mut context);
        }
    }
    context
}

/// Whether `node` sits in the ref-less static-text run right after a field
/// that can hold typed text: the shape a token field's chip labels take,
/// mirroring [`crate::agentic::flow::ask::detokenize`]'s own selection of the
/// nodes it reads as a field's tokens.
fn follows_a_settable_field(ordered: &[&Candidate], node: &Candidate) -> bool {
    let Some(position) = ordered
        .iter()
        .position(|candidate| candidate.order == node.order)
    else {
        return false;
    };
    let is_static_text = |candidate: &Candidate| {
        candidate.ref_id.is_empty() && candidate.role.eq_ignore_ascii_case("statictext")
    };
    let mut start = position;
    while start > 0 && is_static_text(ordered[start - 1]) {
        start -= 1;
    }
    start > 0
        && ordered[start - 1]
            .available_actions
            .iter()
            .any(|action| action == "SetValue" || action == "TypeText")
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

impl Surface for Desktop {
    fn observe(
        &self,
        app: &str,
        root: Option<&str>,
        depth: Depth,
    ) -> Result<Screen, Box<DesktopResponse>> {
        observe(self, app, root, depth)
    }

    fn execute(
        &self,
        operation: JevOperation,
        target: Option<Candidate>,
        text: Option<String>,
    ) -> DesktopResponse {
        execute_desktop(self, operation, target.as_ref(), text)
    }

    fn read_value(&self, target: &Candidate) -> Option<String> {
        // A plain field answers with its value; a rich-text area often has
        // none and answers with its text instead.
        [ElementProperty::Value, ElementProperty::Text]
            .into_iter()
            .find_map(|property| {
                let reply = self.get(GetRequest::new(target.ref_id.clone(), property));
                reply
                    .data
                    .as_ref()
                    .filter(|_| reply.ok)
                    .and_then(|data| data.get("value").and_then(Value::as_str))
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_owned)
            })
    }

    fn paste(&self, app: &str, target: &Candidate, text: &str) -> DesktopResponse {
        let previous = self.clipboard_get(ClipboardGetRequest {
            format: Some(ClipboardFormat::Auto),
            out: None,
        });
        let focused = self.focus(RefRequest::new(target.ref_id.clone()));
        if !focused.ok {
            let clicked = self.click(RefRequest::new(target.ref_id.clone()));
            if !clicked.ok {
                return clicked;
            }
        }
        let staged = self.clipboard_set(ClipboardSetRequest::text(text));
        if !staged.ok {
            return staged;
        }
        let replaces = target
            .available_actions
            .iter()
            .any(|action| action == "SetValue");
        let selected = if replaces {
            self.press(press_at(app, "cmd+a"))
        } else {
            DesktopResponse::ok("press", json!({}))
        };
        let pasted = self.press(press_at(app, "cmd+v"));
        let _settled = self.wait(WaitRequest::sleep(150));
        let restored = restore_plan(&previous).is_none_or(|restore| {
            match restore {
                Restore::Set(request) => self.clipboard_set(request),
                Restore::Clear => self.clipboard_clear(),
            }
            .ok
        });
        with_restoration(if selected.ok { pasted } else { selected }, restored)
    }

    fn press(&self, app: &str, combo: &str) -> DesktopResponse {
        Desktop::press(self, press_at(app, combo))
    }

    fn launch(&self, app: &str) -> DesktopResponse {
        let mut request = LaunchRequest::new(app);
        request.activate = true;
        running_is_launched(Desktop::launch(self, request))
    }

    fn settle(&self) {
        let _settled = self.wait(WaitRequest::sleep(SETTLE_MS));
    }
}

/// How long a field is given to commit text before it is read back again.
const SETTLE_MS: u64 = 200;

/// What to put back on the pasteboard once a paste is done with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Restore {
    /// Write this back; it is what the pasteboard held before the paste.
    Set(ClipboardSetRequest),
    /// The pasteboard held nothing in a readable flavor before the paste, so
    /// it is emptied rather than left holding the field text the paste staged.
    Clear,
}

/// Decides how to restore the pasteboard from what a pre-paste
/// [`Desktop::clipboard_get`] read there, without touching the pasteboard
/// itself: a pure function a test can drive with a scripted reply.
///
/// A failed read (permission denied, no pasteboard service) reports nothing
/// unreadable rather than "empty", so the pasteboard is left untouched instead
/// of being cleared on a guess.
pub(crate) fn restore_plan(previous: &DesktopResponse) -> Option<Restore> {
    let data = previous.data.as_ref().filter(|_| previous.ok)?;
    if data.get("found").and_then(Value::as_bool) == Some(false) {
        return Some(Restore::Clear);
    }
    match data.get("type").and_then(Value::as_str) {
        Some("text") => data
            .get("text")
            .and_then(Value::as_str)
            .map(|text| Restore::Set(ClipboardSetRequest::text(text))),
        Some("file_urls") => data.get("file_urls").and_then(Value::as_array).map(|urls| {
            let file_urls = urls
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            Restore::Set(ClipboardSetRequest {
                file_urls,
                ..ClipboardSetRequest::default()
            })
        }),
        Some("image") => data.get("path").and_then(Value::as_str).map(|path| {
            Restore::Set(ClipboardSetRequest {
                image: Some(path.to_owned()),
                ..ClipboardSetRequest::default()
            })
        }),
        // `found: false`, or an unrecognized flavor: the pasteboard had
        // nothing readable before the paste, so it is emptied rather than
        // left holding the field text the paste just staged there.
        _ => Some(Restore::Clear),
    }
}

/// Folds a failed clipboard restoration into the response `paste` would
/// otherwise return, instead of discarding it.
///
/// The field operation's own success is left alone — the text still arrived,
/// and a caller checking `ok` must keep seeing that — but `data` gains
/// `clipboard_restored: false` so a caller that reads it can see the user's
/// prior clipboard contents were not put back. An already-failed response has
/// nothing to fold into and is returned unchanged.
fn with_restoration(mut result: DesktopResponse, restored: bool) -> DesktopResponse {
    if !restored && result.ok {
        let mut data = result.data.take().unwrap_or_else(|| json!({}));
        if let Value::Object(map) = &mut data {
            map.insert("clipboard_restored".to_owned(), json!(false));
        }
        result.data = Some(data);
    }
    result
}

/// Treats "several windows match" as launched: the application is running,
/// and which of its windows to act in is the next step's decision.
pub(crate) fn running_is_launched(reply: DesktopResponse) -> DesktopResponse {
    if reply
        .error
        .as_ref()
        .is_some_and(|error| error.code == "AMBIGUOUS_TARGET")
    {
        return DesktopResponse::ok(
            &reply.command,
            json!({"running": true, "windows": "several"}),
        );
    }
    reply
}

/// Rewrites the platform-neutral `cmd` modifier a flow's shortcuts and this
/// module's paste path both write into whatever modifier this platform
/// actually binds for an application's primary shortcuts.
///
/// The engine's combo parser maps `cmd` literally to the Meta key
/// (`vendor/agent-desktop/crates/core/src/commands/combo.rs`), which is the
/// Command key on macOS but the Windows/Super key everywhere else, so a
/// shortcut such as `cmd+n` would open the Start menu instead of a new item
/// on Windows and Linux. `is_macos` is threaded through rather than read from
/// `cfg!` here so the mapping stays a pure, unit-testable function; the one
/// caller below supplies the real platform.
pub(crate) fn platform_combo(combo: &str, is_macos: bool) -> String {
    if is_macos {
        return combo.to_owned();
    }
    combo
        .split('+')
        .map(|part| if part == "cmd" { "ctrl" } else { part })
        .collect::<Vec<_>>()
        .join("+")
}

fn press_at(app: &str, combo: &str) -> PressRequest {
    let mut request = PressRequest::new(platform_combo(combo, cfg!(target_os = "macos")));
    request.app = Some(app.to_owned());
    request
}

pub(crate) fn execute_desktop(
    desktop: &Desktop,
    operation: JevOperation,
    target: Option<&Candidate>,
    text: Option<String>,
) -> DesktopResponse {
    let ref_id = target.map(|node| node.ref_id.clone());
    match operation {
        JevOperation::Click => desktop.click(RefRequest::new(ref_id.unwrap_or_default())),
        JevOperation::TypeText => desktop.set_value(SetValueRequest {
            ref_id: ref_id.unwrap_or_default(),
            value: text.unwrap_or_default(),
            ..SetValueRequest::default()
        }),
        JevOperation::Check => desktop.check(RefRequest::new(ref_id.unwrap_or_default())),
        JevOperation::Uncheck => desktop.uncheck(RefRequest::new(ref_id.unwrap_or_default())),
        JevOperation::Expand => desktop.expand(RefRequest::new(ref_id.unwrap_or_default())),
        JevOperation::Collapse => desktop.collapse(RefRequest::new(ref_id.unwrap_or_default())),
        JevOperation::Scroll => desktop.scroll(ScrollRequest::new(
            ref_id.unwrap_or_default(),
            Direction::Down,
            3,
        )),
        JevOperation::Wait => desktop.wait(WaitRequest::sleep(500)),
        JevOperation::Drill | JevOperation::Widen => {
            DesktopResponse::ok("look", json!({"root": ref_id}))
        }
        JevOperation::Done | JevOperation::Blocked => {
            DesktopResponse::ok("resolve-intent", json!({}))
        }
    }
}

#[cfg(test)]
mod test;
