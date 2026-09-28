//! [`Desktop`] as a [`Surface`]: agent-desktop snapshots parsed into what a
//! decision loop sees, and the closed operations it acts with.
//!
//! Observation keeps static text as context, records subtrees the engine cut
//! short, keeps field content out of that shared context, and resolves an
//! application with several windows to its front one. Paste goes through the
//! clipboard and puts back what it held.

use serde_json::{Value, json};
use tinycomputer_bus::{
    ClipboardFormat, ClipboardGetRequest, ClipboardSetRequest, DesktopResponse, Direction,
    ElementProperty, GetRequest, JevOperation, LaunchRequest, ListWindowsRequest, PressRequest,
    RefRequest, ScrollRequest, SetValueRequest, SnapshotRequest, Surface as Overlay, WaitRequest,
};
use tinycomputer_core::surface::{Candidate, Depth, Screen, Surface, uses_pointer};
use tinycomputer_cursor::Rect;

mod act;
mod observation;
mod paste;

use act::{execute_desktop, press_at, running_is_launched};
use observation::observe;
use paste::{Restore, restore_plan, with_restoration};

use crate::Desktop;

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

#[cfg(test)]
mod surface_tests;
