//! Whether the focused element, or the one a ref names, takes typed text.

use super::{BrowserSurface, sight};
use super::envelope::not_a_text_field;
use super::operations::target;

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
use super::BrowserSurface;

impl BrowserSurface {
    /// Whether the page's currently focused element takes typed text: a
    /// text-like `<input>` that is neither read-only nor disabled, a
    /// `<textarea>`, or a `contenteditable` region. An ARIA role alone does
    /// not count: pages give `combobox` and `textbox` roles to list rows and
    /// buttons that hold no text (measured on a booking widget, whose city
    /// rows are `div role="combobox"`). Typing without a target sends keys
    /// wherever the browser's own focus happens to be, so this is checked
    /// first: a stale or unexpected focus — an unrelated field, or none at
    /// all — must never silently receive text, including a private value.
    pub(super) fn focused_field_is_editable(&self) -> bool {
        const SCRIPT: &str = r"(() => {
  // Focusing a ref inside an open shadow root or a same-origin frame — the
  // documented tree fallback's territory — leaves `document.activeElement`
  // pointing at the shadow host or the `<iframe>` itself, not the nested
  // field that actually holds the focus; descend into both before judging
  // editability, so that documented fallback stays usable for text entry.
  const deepActiveElement = () => {
    let element = document.activeElement;
    for (;;) {
      if (element && element.shadowRoot && element.shadowRoot.activeElement) {
        element = element.shadowRoot.activeElement;
        continue;
      }
      if (element && element.tagName === 'IFRAME') {
        try {
          const inner = element.contentDocument && element.contentDocument.activeElement;
          if (inner) {
            element = inner;
            continue;
          }
        } catch (e) {
          // Cross-origin frame: inaccessible, judge the host element itself.
        }
      }
      return element;
    }
  };
  const element = deepActiveElement();
  if (!element) return false;
  const tag = (element.tagName || '').toLowerCase();
  if (element.isContentEditable) return true;
  if (tag === 'textarea') return !element.readOnly && !element.disabled;
  if (tag !== 'input') return false;
  const type = (element.getAttribute('type') || 'text').toLowerCase();
  return ['text', 'search', 'email', 'tel', 'url', 'number', 'password',
    'date', 'time', 'month', 'week', 'datetime-local'].includes(type)
    && !element.readOnly && !element.disabled;
})()";
        let Ok(id) = self.ensure_session() else {
            return false;
        };
        self.block(
            self.browser
                .command(&id, json!({"action": "evaluate", "script": SCRIPT})),
        )
        .ok()
        .and_then(|data| data.get("result").and_then(Value::as_bool))
        .unwrap_or(false)
    }

    /// Whether the element `reference` names takes typed text: focusing it
    /// lands on an input, a text area, an editable region, or a text-box
    /// role. A page can give any `div` a `combobox` or `textbox` role — a
    /// city in a list of suggestions, a card — and a fill or a paste into
    /// one reports success while nothing holds the text.
    pub(super) fn takes_text(&self, reference: &str) -> bool {
        self.perform(
            "focus",
            Action::Focus {
                target: target(reference),
            },
        )
        .ok && self.focused_field_is_editable()
    }
}
