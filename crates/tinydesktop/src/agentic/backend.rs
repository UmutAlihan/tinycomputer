//! The desktop operations the Jev loops drive, behind a trait tests can fake.
//!
//! Everything that touches the engine goes through [`AgentBackend`], so the
//! loops above it are deterministic Rust that a unit test can script end to
//! end. Text delivery lives here too: it is the one operation that needs a
//! verify-and-fall-back sequence rather than a single engine call.

use serde_json::{Value, json};
use tinydesktop_bus::{
    ClipboardGetRequest, ClipboardSetRequest, DesktopResponse, Direction, ElementProperty,
    GetRequest, JevOperation, LaunchRequest, PressRequest, RefRequest, ScrollRequest,
    SetValueRequest, WaitRequest,
};

use super::{
    internal_error,
    screen::{Candidate, Depth, Screen, observe},
};
use crate::Desktop;

/// The engine surface the Jev loops depend on.
pub(super) trait AgentBackend: Clone + Send + 'static {
    /// Reads the current surface of `app`, optionally rooted at a container.
    fn observe(
        &self,
        app: &str,
        root: Option<&str>,
        depth: Depth,
    ) -> Result<Screen, Box<DesktopResponse>>;

    /// Runs one closed operation. `TypeText` only sets the value; callers that
    /// need it verified go through [`deliver_text`].
    fn execute(
        &self,
        operation: JevOperation,
        target: Option<Candidate>,
        text: Option<String>,
    ) -> DesktopResponse;

    /// Reads an element's current value, when the platform exposes one.
    fn read_value(&self, target: &Candidate) -> Option<String>;

    /// Focuses `target`, puts `text` into it through the pasteboard, and
    /// restores whatever the pasteboard held before.
    ///
    /// A field that supports set-value is replaced (select all, then paste).
    /// One that does not — a rich-text body such as a mail message — has the
    /// text pasted at the caret, so a reply keeps the message it quotes.
    fn paste(&self, app: &str, target: &Candidate, text: &str) -> DesktopResponse;

    /// Presses a key combination at `app`.
    fn press(&self, app: &str, combo: &str) -> DesktopResponse;

    /// Launches `app`, or brings it forward when it is already running.
    fn launch(&self, app: &str) -> DesktopResponse;

    /// Gives the application a moment to commit what it was just given, as a
    /// token field does when it turns an address into a token.
    fn settle(&self) {}
}

impl AgentBackend for Desktop {
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
        let previous = self
            .clipboard_get(ClipboardGetRequest::default())
            .data
            .and_then(|data| data.get("text").and_then(Value::as_str).map(str::to_owned));
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
        if let Some(previous) = previous {
            let _restored = self.clipboard_set(ClipboardSetRequest::text(previous));
        }
        if selected.ok { pasted } else { selected }
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

/// Treats "several windows match" as launched: the application is running,
/// and which of its windows to act in is the next step's decision.
pub(super) fn running_is_launched(reply: DesktopResponse) -> DesktopResponse {
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

fn press_at(app: &str, combo: &str) -> PressRequest {
    let mut request = PressRequest::new(combo);
    request.app = Some(app.to_owned());
    request
}

pub(super) async fn observe_async<B: AgentBackend>(
    backend: B,
    app: String,
    root: Option<String>,
    depth: Depth,
) -> Result<Screen, Box<DesktopResponse>> {
    tokio::task::spawn_blocking(move || backend.observe(&app, root.as_deref(), depth))
        .await
        .map_err(|error| {
            Box::new(internal_error(&format!(
                "desktop observation task failed: {error}"
            )))
        })?
}

/// Runs a blocking backend call off the async executor.
pub(super) async fn blocking<B, F, T>(backend: B, call: F) -> T
where
    B: AgentBackend,
    F: FnOnce(B) -> T + Send + 'static,
    T: Send + 'static + From<DesktopResponse>,
{
    tokio::task::spawn_blocking(move || call(backend))
        .await
        .unwrap_or_else(|error| {
            T::from(internal_error(&format!(
                "desktop action task failed: {error}"
            )))
        })
}

pub(super) async fn execute_operation<B: AgentBackend>(
    backend: B,
    app: String,
    operation: JevOperation,
    target: Option<Candidate>,
    text: Option<String>,
) -> DesktopResponse {
    if operation == JevOperation::TypeText
        && let (Some(target), Some(text)) = (target.clone(), text.clone())
    {
        return blocking(backend, move |backend| {
            deliver_text(&backend, &app, &target, &text)
        })
        .await;
    }
    blocking(backend, move |backend| {
        backend.execute(operation, target, text)
    })
    .await
}

/// Puts `text` into `target` and proves it arrived.
///
/// The accessibility set-value path is fast and headless but silently no-ops
/// on some fields (rich text bodies, token fields). So the value is read back,
/// and on a mismatch the text is pasted instead and read back again. The reply
/// names which path delivered it; an unreadable field is reported as
/// delivered-but-unverified rather than as a failure.
pub(super) fn deliver_text<B: AgentBackend>(
    backend: &B,
    app: &str,
    target: &Candidate,
    text: &str,
) -> DesktopResponse {
    let set = backend.execute(
        JevOperation::TypeText,
        Some(target.clone()),
        Some(text.to_owned()),
    );
    if set.ok {
        match read_settled(backend, target, text) {
            Some(held) if holds(&held, text) => return delivered("set_value", true),
            Some(held) if tokenized(&held) => return delivered("set_value", false),
            None => return delivered("set_value", false),
            Some(_) => {}
        }
    }
    let pasted = backend.paste(app, target, text);
    if !pasted.ok {
        return if set.ok { pasted } else { set };
    }
    match read_settled(backend, target, text) {
        Some(held) if holds(&held, text) => delivered("paste", true),
        None => delivered("paste", false),
        Some(_) => DesktopResponse::err(
            "type-text",
            tinydesktop_bus::DesktopError::new(
                "TEXT_NOT_DELIVERED",
                "the field did not hold the text after set-value and paste",
            ),
        ),
    }
}

/// Reads `target` back, and once more after [`AgentBackend::settle`] when the
/// first read does not yet hold `text`.
fn read_settled<B: AgentBackend>(backend: &B, target: &Candidate, text: &str) -> Option<String> {
    let first = backend.read_value(target);
    if first.as_deref().is_none_or(|held| holds(held, text)) {
        return first;
    }
    backend.settle();
    backend.read_value(target).or(first)
}

fn delivered(path: &str, verified: bool) -> DesktopResponse {
    DesktopResponse::ok("type-text", json!({"path": path, "verified": verified}))
}

/// Whether a read-back value is only attachment tokens.
///
/// A token field (a mail recipient list) turns each typed address into an
/// attachment and reports it as U+FFFC, the object replacement character, so
/// its value can never be compared with what was typed.
pub(super) fn tokenized(held: &str) -> bool {
    held.contains('\u{fffc}')
        && held.chars().all(|character| {
            character == '\u{fffc}' || character == ',' || character.is_whitespace()
        })
}

/// Whether a field's read-back value carries the delivered text.
///
/// Whitespace is collapsed on both sides: editors rewrap lines and turn a
/// newline into a paragraph break, and neither changes what was written.
pub(super) fn holds(held: &str, text: &str) -> bool {
    let normalize = |value: &str| value.split_whitespace().collect::<Vec<_>>().join(" ");
    let (held, text) = (normalize(held), normalize(text));
    !text.is_empty() && held.contains(&text)
}

pub(super) fn execute_desktop(
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
        JevOperation::ScrollUp => desktop.scroll(ScrollRequest::new(
            ref_id.unwrap_or_default(),
            Direction::Up,
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
