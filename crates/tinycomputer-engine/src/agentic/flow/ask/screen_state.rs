//! The shared state every question about a screen is asked against: its
//! elements, the text fields hold, and the recent history.


use serde_json::{Value, json};

use crate::agentic::flow::view::{Candidate, Screen, element_line, label, untrusted_context};

use super::{MAX_STATE_ELEMENTS, MAX_HISTORY, MAX_FIELDS};

/// The shared state every question about `screen` is asked against.
pub(in crate::agentic::flow) fn state(
    screen: &Screen,
    goal: &str,
    history: &[String],
    include_values: bool,
) -> Value {
    let elements = screen
        .candidates
        .iter()
        .take(MAX_STATE_ELEMENTS)
        .map(|node| element_line(node, include_values))
        .collect::<Vec<_>>();
    let mut state = json!({
        "app": screen.app,
        "window": screen.window,
        "surface": screen.surface,
        "current_step": goal,
        "visible_text": untrusted_context(screen),
        "elements": {"untrusted_accessibility_data": elements},
        "recent_actions": history.iter().rev().take(MAX_HISTORY).rev().collect::<Vec<_>>(),
    });
    if include_values {
        state["field_contents"] = json!({"untrusted_accessibility_data": field_contents(screen)});
    }
    state
}

/// What each text-holding element shows, at more length than the element
/// list allows: whether a draft "shows the body" is decided here.
///
/// A plain field holds its text as its value. A rich-text area (a mail body,
/// a web view) holds none; its text is spread over the static text inside it
/// — ref-less, so it never appears in `screen.candidates` — which is why this
/// reads the merged, document-ordered view over `candidates` and
/// `text_nodes` instead.
fn field_contents(screen: &Screen) -> Vec<Value> {
    let ordered = ordered_nodes(screen);
    let mut fields = Vec::new();
    for node in &screen.candidates {
        let holds_text = node
            .available_actions
            .iter()
            .any(|action| action == "SetValue" || action == "TypeText");
        let own = node
            .value
            .as_ref()
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| holds_text && !value.is_empty())
            .map(|value| detokenize(value, following(&ordered, node.order)));
        let text = own.or_else(|| rich_text(&ordered, node));
        if let Some(text) = text {
            // Not clipped to `MAX_FIELD_CHARS` here: `FlowRun::mask` needs
            // the whole value to find a secret by its exact text, and the
            // runtime clips the masked result afterward instead.
            fields.push(json!({
                "field": label(node),
                "holds": text,
            }));
        }
        if fields.len() >= MAX_FIELDS {
            break;
        }
    }
    fields
}

/// The nodes in `ordered` (candidates and text nodes merged and sorted by
/// [`Candidate::order`]) that follow the node at `order`, in document order.
fn following<'a>(ordered: &'a [&'a Candidate], order: usize) -> &'a [&'a Candidate] {
    let start = ordered.partition_point(|node| node.order <= order);
    &ordered[start..]
}

/// A token field's value with each U+FFFC attachment replaced by the static
/// text that follows the field in document order, which is how the tokens
/// are exposed.
fn detokenize(value: &str, following: &[&Candidate]) -> String {
    if !value.contains('\u{fffc}') {
        return value.to_owned();
    }
    let tokens = following
        .iter()
        .take_while(|node| node.role.eq_ignore_ascii_case("statictext"))
        .filter_map(|node| {
            node.name
                .as_deref()
                .or(node.value.as_ref().and_then(Value::as_str))
        })
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return value.replace('\u{fffc}', "[token]");
    }
    tokens.join(", ")
}

/// The document-ordered merge of `screen`'s candidates and the ref-less text
/// nodes `collect` set aside, which [`rich_text`] and [`detokenize`] both
/// walk to find a field's held text.
pub(in crate::agentic::flow) fn ordered_nodes(screen: &Screen) -> Vec<&Candidate> {
    let mut ordered = screen
        .candidates
        .iter()
        .chain(screen.text_nodes.iter())
        .collect::<Vec<_>>();
    ordered.sort_by_key(|node| node.order);
    ordered
}

/// The text inside a rich-text area, joined in reading order.
pub(in crate::agentic::flow) fn rich_text(ordered: &[&Candidate], area: &Candidate) -> Option<String> {
    if !["webarea", "document"]
        .iter()
        .any(|role| area.role.eq_ignore_ascii_case(role))
    {
        return None;
    }
    let area_label = label(area);
    let text = ordered
        .iter()
        .filter(|node| node.path.contains(&area_label))
        .filter_map(|node| {
            node.value
                .as_ref()
                .and_then(Value::as_str)
                .or(node.name.as_deref())
        })
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then_some(text)
}
