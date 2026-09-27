//! Verified text delivery: set the value, read it back, paste when it did
//! not arrive, and say which path worked.

use serde_json::json;
use tinycomputer_bus::{DesktopResponse, JevOperation};

use super::{Candidate, Surface};

/// Puts `text` into `target` and proves it arrived.
///
/// The accessibility set-value path is fast and headless but silently no-ops
/// on some fields (rich text bodies, token fields). So the value is read back,
/// and on a mismatch the text is pasted instead and read back again. The reply
/// names which path delivered it; an unreadable field is reported as
/// delivered-but-unverified rather than as a failure.
pub fn deliver_text<S: Surface>(
    surface: &S,
    app: &str,
    target: &Candidate,
    text: &str,
) -> DesktopResponse {
    let set = surface.execute(
        JevOperation::TypeText,
        Some(target.clone()),
        Some(text.to_owned()),
    );
    if set.ok {
        match read_settled(surface, target, text) {
            Some(held) if holds(&held, text) => return delivered("set_value", true),
            Some(held) if tokenized(&held) => return delivered("set_value", false),
            None => return delivered("set_value", false),
            Some(_) => {}
        }
    }
    let pasted = surface.paste(app, target, text);
    if !pasted.ok {
        return if set.ok { pasted } else { set };
    }
    match read_settled(surface, target, text) {
        Some(held) if holds(&held, text) => delivered("paste", true),
        None => delivered("paste", false),
        Some(_) => DesktopResponse::err(
            "type-text",
            tinycomputer_bus::DesktopError::new(
                "TEXT_NOT_DELIVERED",
                "the field did not hold the text after set-value and paste",
            ),
        ),
    }
}

/// Reads `target` back, and once more after [`Surface::settle`] when the
/// first read does not yet hold `text`.
fn read_settled<S: Surface>(surface: &S, target: &Candidate, text: &str) -> Option<String> {
    let first = surface.read_value(target);
    if first.as_deref().is_none_or(|held| holds(held, text)) {
        return first;
    }
    surface.settle();
    surface.read_value(target).or(first)
}

fn delivered(path: &str, verified: bool) -> DesktopResponse {
    DesktopResponse::ok("type-text", json!({"path": path, "verified": verified}))
}

/// Whether a read-back value is only attachment tokens.
///
/// A token field (a mail recipient list) turns each typed address into an
/// attachment and reports it as U+FFFC, the object replacement character, so
/// its value can never be compared with what was typed.
#[must_use]
pub fn tokenized(held: &str) -> bool {
    held.contains('\u{fffc}')
        && held.chars().all(|character| {
            character == '\u{fffc}' || character == ',' || character.is_whitespace()
        })
}

/// Whether a field's read-back value carries the delivered text.
///
/// Whitespace is collapsed on both sides: editors rewrap lines and turn a
/// newline into a paragraph break, and neither changes what was written.
#[must_use]
pub fn holds(held: &str, text: &str) -> bool {
    let normalize = |value: &str| value.split_whitespace().collect::<Vec<_>>().join(" ");
    let (held, text) = (normalize(held), normalize(text));
    !text.is_empty() && held.contains(&text)
}
