//! Browser results and errors as the envelope decision loops read.

use super::{BrowserSurface, sight};

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tinycomputer_bus::browser::{
    Action, NavigateRequest, ScrollDirection, SessionId, SessionOptions, SnapshotRequest, Target,
    WaitState,
};
use tinycomputer_bus::{DesktopError, DesktopResponse, JevOperation};
use tinycomputer_core::surface::{Candidate, Depth, Screen, Surface, uses_pointer};
use tinycomputer_core::{Key, Platform};
use tinycomputer_cursor::ScreenCursor;

use crate::error::{Error, Result};
use crate::sessions::Browser;

/// A browser result as the envelope decision loops read.
pub(super) fn reply(command: &str, result: Result<Value>) -> DesktopResponse {
    match result {
        Ok(data) => DesktopResponse::ok(command, data),
        Err(error) => failure(command, &error),
    }
}

/// A browser error as an envelope failure whose code is the error's wire
/// name in `SCREAMING_SNAKE_CASE`, such as `STALE_REF`.
/// Whether a click was refused because another element covers its target.
pub(super) fn covered(reply: &DesktopResponse) -> bool {
    reply
        .error
        .as_ref()
        .is_some_and(|error| error.message.contains("is covered by"))
}

pub(super) fn failure(command: &str, error: &Error) -> DesktopResponse {
    let name = error
        .wire_name()
        .rsplit('.')
        .next()
        .unwrap_or("ModuleFailed");
    let mut code = String::with_capacity(name.len() + 4);
    for (index, character) in name.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            code.push('_');
        }
        code.push(character.to_ascii_uppercase());
    }
    DesktopResponse::err(command, DesktopError::new(code, error.to_string()))
}

/// The refusal for text aimed at an element that does not take it.
pub(super) fn not_a_text_field() -> DesktopResponse {
    DesktopResponse::err(
        "type-text",
        DesktopError::new(
            "NOT_A_TEXT_FIELD",
            "the element does not take typed text: focusing it reaches no input, text area, or editable region",
        ),
    )
}
