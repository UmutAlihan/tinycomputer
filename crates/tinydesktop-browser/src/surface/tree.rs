//! agent-browser's snapshot text as a decision loop's [`Screen`].
//!
//! Each line is `- role "name" [attr, attr=value, ref=eN]: value`, indented
//! two spaces per level. Lines with a ref become candidates; readable lines
//! without one — headings, paragraphs, static text — become context, the way
//! labels and status text do on the desktop.

use serde_json::Value;
use tinydesktop_core::surface::{Candidate, Screen};

/// The most context lines kept, and the longest one.
const MAX_CONTEXT_LINES: usize = 60;
const MAX_CONTEXT_CHARS: usize = 160;

/// Roles that contain a page's field content rather than its chrome; text
/// inside them never reaches the shared context.
const FIELD_ROLES: &[&str] = &["textbox", "searchbox", "combobox", "textarea", "document"];

/// Roles a value is typed into.
const TYPED_ROLES: &[&str] = &["textbox", "searchbox", "combobox", "spinbutton", "textarea"];

/// Roles that are toggled.
const TOGGLED_ROLES: &[&str] = &[
    "checkbox",
    "radio",
    "switch",
    "menuitemcheckbox",
    "menuitemradio",
];

/// One parsed snapshot line.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Line {
    pub(crate) depth: usize,
    pub(crate) role: String,
    pub(crate) name: Option<String>,
    pub(crate) attributes: Vec<(String, Option<String>)>,
    pub(crate) value: Option<String>,
}

impl Line {
    /// Whether the attribute is present, as a flag or with a value.
    fn has(&self, key: &str) -> bool {
        self.attributes.iter().any(|(name, _)| name == key)
    }

    /// The attribute's value, when it has one.
    fn value_of(&self, key: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(name, _)| name == key)
            .and_then(|(_, value)| value.as_deref())
    }

    fn reference(&self) -> Option<&str> {
        self.value_of("ref")
    }

    fn label(&self) -> String {
        self.name.as_deref().map_or_else(
            || self.role.clone(),
            |name| format!("{} {name:?}", self.role),
        )
    }
}

/// Parses one snapshot line; `None` for anything that is not an item.
#[must_use]
pub(crate) fn parse_line(text: &str) -> Option<Line> {
    let trimmed = text.trim_start_matches(' ');
    let depth = (text.len() - trimmed.len()) / 2;
    let rest = trimmed.strip_prefix("- ")?;
    let role_end = rest.find([' ', ':']).unwrap_or(rest.len());
    let role = rest[..role_end].to_owned();
    let mut rest = &rest[role_end..];
    let mut line = Line {
        depth,
        role,
        ..Line::default()
    };
    if let Some(quoted) = rest.strip_prefix(" \"") {
        let end = closing_quote(quoted)?;
        line.name = serde_json::from_str(&format!("\"{}\"", &quoted[..end])).ok();
        rest = &quoted[end + 1..];
    }
    if let Some(bracketed) = rest.strip_prefix(" [") {
        let end = bracketed.find(']')?;
        line.attributes = bracketed[..end]
            .split(", ")
            .filter(|part| !part.is_empty())
            .map(|part| match part.split_once('=') {
                Some((key, value)) => (key.to_owned(), Some(value.to_owned())),
                None => (part.to_owned(), None),
            })
            .collect();
        rest = &bracketed[end + 1..];
    }
    if let Some((_, value)) = rest.split_once(": ") {
        line.value = Some(value.to_owned());
    } else if let Some(value) = rest.strip_prefix(':') {
        line.value = Some(value.trim().to_owned()).filter(|value| !value.is_empty());
    }
    Some(line)
}

/// The index of the unescaped quote that closes a JSON string body.
fn closing_quote(body: &str) -> Option<usize> {
    let mut escaped = false;
    for (index, character) in body.char_indices() {
        match character {
            '\\' if !escaped => escaped = true,
            '"' if !escaped => return Some(index),
            _ => escaped = false,
        }
    }
    None
}

/// A whole snapshot as a [`Screen`] of the page titled `title`.
#[must_use]
pub(crate) fn screen(tree: &str, title: &str) -> Screen {
    let mut candidates = Vec::new();
    let mut context: Vec<String> = Vec::new();
    let mut text_nodes = Vec::new();
    let mut surface = "window".to_owned();
    // Labels of the open ancestors at each depth.
    let mut ancestors: Vec<(usize, String, String)> = Vec::new();
    for (order, line) in tree.lines().filter_map(parse_line).enumerate() {
        ancestors.retain(|(depth, _, _)| *depth < line.depth);
        let path = ancestors
            .iter()
            .map(|(_, label, _)| label.clone())
            .collect::<Vec<_>>();
        let inside_field = ancestors
            .iter()
            .any(|(_, _, role)| FIELD_ROLES.contains(&role.as_str()));
        match line.role.as_str() {
            "dialog" if surface == "window" => "sheet".clone_into(&mut surface),
            "alertdialog" => "alert".clone_into(&mut surface),
            _ => {}
        }
        let node = candidate(&line, path, order);
        if line.reference().is_some() {
            if !line.has("disabled") {
                candidates.push(node);
            }
        } else {
            if !inside_field {
                remember(&line, &mut context);
            }
            text_nodes.push(node);
        }
        ancestors.push((line.depth, line.label(), line.role));
    }
    Screen {
        app: "browser".to_owned(),
        window: Some(title.to_owned()).filter(|title| !title.is_empty()),
        surface,
        candidates,
        context,
        unexplored: Vec::new(),
        text_nodes,
    }
}

fn candidate(line: &Line, path: Vec<String>, order: usize) -> Candidate {
    let states = ["checked", "expanded", "selected", "required"]
        .into_iter()
        .filter(|state| {
            line.value_of(state)
                .map_or_else(|| line.has(state), |value| value == "true")
        })
        .map(str::to_owned)
        .collect();
    let available_actions = if TYPED_ROLES.contains(&line.role.as_str()) {
        vec!["Click".to_owned(), "SetValue".to_owned()]
    } else if TOGGLED_ROLES.contains(&line.role.as_str()) {
        vec!["Click".to_owned(), "Check".to_owned()]
    } else {
        vec!["Click".to_owned()]
    };
    Candidate {
        ref_id: line.reference().unwrap_or_default().to_owned(),
        role: line.role.clone(),
        name: line.name.clone().filter(|name| !name.is_empty()),
        value: line.value.clone().map(Value::String),
        states,
        available_actions,
        path,
        order,
        ..Candidate::default()
    }
}

fn remember(line: &Line, context: &mut Vec<String>) {
    if context.len() >= MAX_CONTEXT_LINES {
        return;
    }
    let Some(text) = line.name.as_deref().or(line.value.as_deref()) else {
        return;
    };
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return;
    }
    let text: String = text.chars().take(MAX_CONTEXT_CHARS).collect();
    if !context.contains(&text) {
        context.push(text);
    }
}
