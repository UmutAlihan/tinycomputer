//! Sight: a web page read the way a person looks at it, rather than
//! through its accessibility tree.
//!
//! The accessibility tree says what a page *declares*: roles and names from
//! ARIA and label markup, which most sites apply partly or wrongly. A list of
//! cities marked `combobox` with a label that points nowhere, an icon button
//! with no name, a field labelled only by the words printed above it — the
//! tree leaves each of these unnamed or mislabelled, and a flow cannot tell
//! them apart. A person never reads the markup. They see what is drawn and
//! what is on top, read the words on a control or beside a box, and know a
//! box takes text because it has a caret.
//!
//! `sight.js` does the same in one pass over the rendered page, run through
//! the engine's `evaluate`:
//!
//! - **What counts as a control** is decided by behaviour: native controls,
//!   elements that show a pointer cursor or take focus, and ARIA roles — but
//!   a "text box" that takes no text is a button to press, and one that only
//!   wraps a real input is read as that input.
//! - **Only what is drawn** is kept: no zero-size, hidden, or transparent
//!   elements. An element outside the viewport is `offscreen`; one whose
//!   middle is under something else is `covered` (not when the cover is the
//!   same result card's own text).
//! - **Names are the words a person reads:** the text on the control; for a
//!   field, its tied label, then the page's label for it, then the words
//!   beside or above it, then its placeholder; for a picture-only control,
//!   its alternative text, then the icon's class words (`close`, `search`),
//!   then, for a link, where it leads.
//! - **Two elements drawn as one control are one control:** the same box,
//!   a wrapper with the same words, or a page's own radio drawn next to the
//!   real one in the same label.
//! - **Containers are what a person sees a control in:** dialogs and fixed
//!   layers, landmarks, named sections, and the cards of a list with their
//!   ordinal, so result cards group as they do from the tree.
//!
//! Each control is marked with a `data-tc-seen` attribute the first time it
//! is seen, and keeps it for as long as the element lives: a ref
//! (`seen:12`) is that attribute's CSS selector, so an element a page
//! removes or replaces leaves its ref pointing at nothing, and acting on it
//! fails rather than reaching whatever took its place.
//!
//! Sight gives way to the tree when it cannot reach what it sees: a control
//! inside a shadow root, or a large frame in front, which a CSS selector from
//! the page cannot address.

use serde::Deserialize;
use serde_json::{Value, json};
use tinycomputer_core::surface::{Candidate, Screen};

/// The script, called as `(root, limits)`.
const SIGHT_JS: &str = include_str!("sight.js");

/// The prefix of a ref sight minted.
const PREFIX: &str = "seen:";

/// The most controls one reading returns, in page order.
const MAX_CONTROLS: usize = 800;
/// The most text blocks one reading returns: the viewport and a screen
/// either side.
const MAX_TEXTS: usize = 400;
/// The most visible words considered as a field's label.
const MAX_LABELS: usize = 3_000;
/// The longest name a control is given, and the longest text block.
const MAX_NAME: usize = 120;
const MAX_TEXT: usize = 160;
/// The most context lines kept, as the tree keeps.
const MAX_CONTEXT_LINES: usize = 60;

/// One reading of a page, as `sight.js` returns it.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Reading {
    ok: bool,
    title: String,
    surface: String,
    unreachable: usize,
    nodes: Vec<Node>,
}

/// A control (with an `id`) or a block of text (with `text`).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Node {
    id: Option<String>,
    role: String,
    name: String,
    description: String,
    value: String,
    states: Vec<String>,
    #[serde(rename = "box")]
    bounds: Vec<f64>,
    path: Vec<String>,
    text: Option<String>,
}

/// The expression that reads the page, or the part of it under `root` — a
/// ref from an earlier screen.
#[must_use]
pub(crate) fn script(root: Option<&str>) -> String {
    let root = root.map_or(Value::Null, |reference| Value::String(selector(reference)));
    let limits = json!({
        "controls": MAX_CONTROLS,
        "texts": MAX_TEXTS,
        "labels": MAX_LABELS,
        "name": MAX_NAME,
        "text": MAX_TEXT,
    });
    format!("({})({root}, {limits})", SIGHT_JS.trim().trim_end_matches(';'))
}

/// Whether `reference` was minted by sight.
#[must_use]
pub(crate) fn is_seen(reference: &str) -> bool {
    reference.starts_with(PREFIX)
}

/// How the engine addresses `reference`: sight's refs as the CSS selector
/// of their mark, the tree's as `@eN`.
#[must_use]
pub(crate) fn selector(reference: &str) -> String {
    reference.strip_prefix(PREFIX).map_or_else(
        || format!("@{}", reference.trim_start_matches('@')),
        |id| format!("[data-tc-seen={}]", Value::String(id.to_owned())),
    )
}

/// What `evaluate` returned as a [`Screen`]; `None` when the reading failed
/// or saw a control it cannot reach, so the tree is read instead.
#[must_use]
pub(crate) fn screen(result: &Value) -> Option<Screen> {
    let reading = Reading::deserialize(result).ok()?;
    if !reading.ok || reading.unreachable > 0 {
        return None;
    }
    let mut candidates = Vec::new();
    let mut text_nodes = Vec::new();
    let mut context = Vec::new();
    for (order, node) in reading.nodes.into_iter().enumerate() {
        if let Some(text) = node.text {
            if context.len() < MAX_CONTEXT_LINES && !context.contains(&text) {
                context.push(text.clone());
            }
            text_nodes.push(Candidate {
                role: "text".to_owned(),
                name: Some(text),
                path: node.path,
                order,
                ..Candidate::default()
            });
        } else if let Some(id) = node.id {
            candidates.push(control(node, format!("{PREFIX}{id}"), order));
        }
    }
    Some(Screen {
        app: "browser".to_owned(),
        window: Some(reading.title).filter(|title| !title.is_empty()),
        surface: if reading.surface.is_empty() {
            "window".to_owned()
        } else {
            reading.surface
        },
        candidates,
        context,
        unexplored: Vec::new(),
        text_nodes,
    })
}

fn control(node: Node, ref_id: String, order: usize) -> Candidate {
    let available_actions = match node.role.as_str() {
        "textbox" | "searchbox" => vec!["Click".to_owned(), "SetValue".to_owned()],
        "checkbox" | "radio" | "switch" => vec!["Click".to_owned(), "Check".to_owned()],
        _ => vec!["Click".to_owned()],
    };
    let bounds = match node.bounds[..] {
        [x, y, width, height] => Some(json!({"x": x, "y": y, "width": width, "height": height})),
        _ => None,
    };
    Candidate {
        ref_id,
        role: node.role,
        name: Some(node.name).filter(|name| !name.is_empty()),
        description: Some(node.description).filter(|description| !description.is_empty()),
        value: Some(node.value)
            .filter(|value| !value.is_empty())
            .map(Value::String),
        states: node.states,
        available_actions,
        bounds,
        path: node.path,
        order,
        ..Candidate::default()
    }
}

#[cfg(test)]
mod test;
