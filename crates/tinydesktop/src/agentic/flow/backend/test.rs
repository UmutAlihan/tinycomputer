//! Tests for text delivery and the desktop backend's fail-closed paths.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use serde_json::json;
use tinydesktop_bus::{DesktopResponse, JevOperation};

use super::{
    super::view::{Candidate, Depth, Screen},
    AgentBackend, Restore, blocking, deliver_text, execute_desktop, holds, platform_combo,
    restore_plan, running_is_launched, tokenized, with_restoration,
};

/// A backend whose reads, set-values, and pastes are scripted.
#[derive(Clone, Default)]
struct TextBackend {
    fail_execute: bool,
    fail_paste: bool,
    /// Values `read_value` returns, in order; exhausted means unreadable.
    reads: Arc<Mutex<VecDeque<String>>>,
    pastes: Arc<Mutex<Vec<String>>>,
}

impl AgentBackend for TextBackend {
    fn observe(
        &self,
        _app: &str,
        _root: Option<&str>,
        _depth: Depth,
    ) -> Result<Screen, Box<DesktopResponse>> {
        Err(Box::new(DesktopResponse::err(
            "snapshot",
            tinydesktop_bus::DesktopError::new("EMPTY", "no screen"),
        )))
    }

    fn execute(
        &self,
        _operation: JevOperation,
        _target: Option<Candidate>,
        _text: Option<String>,
    ) -> DesktopResponse {
        if self.fail_execute {
            DesktopResponse::err(
                "fake",
                tinydesktop_bus::DesktopError::new("ACTION_FAILED", "fake failure"),
            )
        } else {
            DesktopResponse::ok("fake", json!({}))
        }
    }

    fn read_value(&self, _target: &Candidate) -> Option<String> {
        self.reads.lock().unwrap().pop_front()
    }

    fn paste(&self, _app: &str, _target: &Candidate, text: &str) -> DesktopResponse {
        self.pastes.lock().unwrap().push(text.to_owned());
        if self.fail_paste {
            DesktopResponse::err(
                "paste",
                tinydesktop_bus::DesktopError::new("PASTE_FAILED", "fake paste failure"),
            )
        } else {
            DesktopResponse::ok("paste", json!({}))
        }
    }

    fn press(&self, _app: &str, _combo: &str) -> DesktopResponse {
        DesktopResponse::ok("press", json!({}))
    }

    fn launch(&self, _app: &str) -> DesktopResponse {
        DesktopResponse::ok("launch", json!({}))
    }
}

fn field() -> Candidate {
    Candidate {
        ref_id: "@s:e1".to_owned(),
        role: "textfield".to_owned(),
        name: Some("Subject".to_owned()),
        available_actions: vec!["SetValue".to_owned()],
        ..Candidate::default()
    }
}

fn reading(reads: &[&str]) -> TextBackend {
    TextBackend {
        reads: Arc::new(Mutex::new(
            reads.iter().map(|read| (*read).to_owned()).collect(),
        )),
        ..TextBackend::default()
    }
}

#[test]
fn text_verified_by_read_back_is_not_pasted() {
    let backend = reading(&["Hello   there"]);
    let reply = deliver_text(&backend, "Mail", &field(), "Hello there");
    assert_eq!(reply.data.unwrap()["path"], json!("set_value"));
    assert!(backend.pastes.lock().unwrap().is_empty());
}

#[test]
fn a_field_that_commits_late_is_verified_on_the_settled_re_read() {
    let backend = reading(&["sam@exa", "sam@example.com"]);
    let reply = deliver_text(&backend, "Mail", &field(), "sam@example.com");
    assert_eq!(reply.data.unwrap()["path"], json!("set_value"));
    assert!(backend.pastes.lock().unwrap().is_empty());
}

#[test]
fn a_token_field_is_delivered_unverified_rather_than_pasted_over() {
    let backend = reading(&["\u{fffc}", "\u{fffc}, \u{fffc}"]);
    let data = deliver_text(&backend, "Mail", &field(), "sam@example.com")
        .data
        .unwrap();
    assert_eq!(
        (data["path"].clone(), data["verified"].clone()),
        (json!("set_value"), json!(false))
    );
    assert!(backend.pastes.lock().unwrap().is_empty());
    assert!(!tokenized("plain"));
}

#[test]
fn a_silently_ignored_set_value_falls_back_to_paste() {
    let backend = reading(&["", "", "Dear Sam, see you Friday"]);
    let data = deliver_text(&backend, "Mail", &field(), "Dear Sam, see you Friday")
        .data
        .unwrap();
    assert_eq!(
        (data["path"].clone(), data["verified"].clone()),
        (json!("paste"), json!(true))
    );
    assert_eq!(backend.pastes.lock().unwrap().len(), 1);
}

#[test]
fn text_that_never_arrives_is_reported_as_not_delivered() {
    let backend = reading(&["", "", "still empty", "still empty"]);
    assert_eq!(
        deliver_text(&backend, "Mail", &field(), "Body")
            .error
            .unwrap()
            .code,
        "TEXT_NOT_DELIVERED"
    );
    let unreadable = reading(&[]);
    assert_eq!(
        deliver_text(&unreadable, "Mail", &field(), "Body")
            .data
            .unwrap()["verified"],
        json!(false)
    );
    let failing = TextBackend {
        fail_execute: true,
        fail_paste: true,
        ..TextBackend::default()
    };
    assert_eq!(
        deliver_text(&failing, "Mail", &field(), "Body")
            .error
            .unwrap()
            .code,
        "ACTION_FAILED"
    );
    let paste_after_set = TextBackend {
        reads: Arc::new(Mutex::new(VecDeque::from(["x".to_owned()]))),
        fail_paste: true,
        ..TextBackend::default()
    };
    assert_eq!(
        deliver_text(&paste_after_set, "Mail", &field(), "Body")
            .error
            .unwrap()
            .code,
        "PASTE_FAILED"
    );
    let set_failed_paste_unverified = TextBackend {
        fail_execute: true,
        ..TextBackend::default()
    };
    assert!(deliver_text(&set_failed_paste_unverified, "Mail", &field(), "Body").ok);
    assert!(!holds("anything", "   "));
}

#[test]
fn restore_plan_writes_back_whatever_flavor_the_pasteboard_held() {
    let text = DesktopResponse::ok("clipboard-get", json!({"type": "text", "text": "hello"}));
    assert_eq!(
        restore_plan(&text),
        Some(Restore::Set(tinydesktop_bus::ClipboardSetRequest::text(
            "hello"
        )))
    );

    let file_urls = DesktopResponse::ok(
        "clipboard-get",
        json!({"type": "file_urls", "file_urls": ["file:///tmp/a.txt"]}),
    );
    assert_eq!(
        restore_plan(&file_urls),
        Some(Restore::Set(tinydesktop_bus::ClipboardSetRequest {
            file_urls: vec!["file:///tmp/a.txt".to_owned()],
            ..tinydesktop_bus::ClipboardSetRequest::default()
        }))
    );

    let image = DesktopResponse::ok(
        "clipboard-get",
        json!({"type": "image", "path": "/tmp/clip.png"}),
    );
    assert_eq!(
        restore_plan(&image),
        Some(Restore::Set(tinydesktop_bus::ClipboardSetRequest {
            image: Some("/tmp/clip.png".to_owned()),
            ..tinydesktop_bus::ClipboardSetRequest::default()
        }))
    );
}

#[test]
fn restore_plan_clears_an_originally_empty_pasteboard() {
    let empty = DesktopResponse::ok("clipboard-get", json!({"type": "text", "found": false}));
    assert_eq!(restore_plan(&empty), Some(Restore::Clear));
}

#[test]
fn restore_plan_leaves_the_pasteboard_alone_when_the_read_failed() {
    let failed = DesktopResponse::err(
        "clipboard-get",
        tinydesktop_bus::DesktopError::new("PERM_DENIED", "no automation permission"),
    );
    assert_eq!(restore_plan(&failed), None);
}

#[test]
fn platform_combo_keeps_cmd_on_macos_and_maps_it_to_ctrl_elsewhere() {
    // Flows and the paste path only ever write app shortcuts in terms of
    // `cmd`; the engine's own combo parser maps that literally to the Meta
    // key, which is the Windows/Super key everywhere but macOS.
    assert_eq!(platform_combo("cmd+n", true), "cmd+n");
    assert_eq!(platform_combo("cmd+shift+n", true), "cmd+shift+n");
    assert_eq!(platform_combo("cmd+n", false), "ctrl+n");
    assert_eq!(platform_combo("cmd+shift+n", false), "ctrl+shift+n");
    assert_eq!(platform_combo("cmd+a", false), "ctrl+a");
    assert_eq!(platform_combo("cmd+v", false), "ctrl+v");
    // Combos with no `cmd` modifier are left untouched on every platform.
    assert_eq!(platform_combo("escape", false), "escape");
    assert_eq!(platform_combo("tab", true), "tab");
}

#[test]
fn a_failed_clipboard_restoration_is_folded_into_the_response_instead_of_ignored() {
    let ok = DesktopResponse::ok("press", json!({}));
    // A successful restoration leaves the response untouched.
    assert_eq!(with_restoration(ok.clone(), true), ok);
    // A failed restoration is reported, not silently dropped, without
    // turning a delivered field's own success into a failure.
    let annotated = with_restoration(ok, false);
    assert!(annotated.ok, "the field operation itself still succeeded");
    assert_eq!(
        annotated.data,
        Some(json!({"clipboard_restored": false}))
    );
    // A failed operation's own error is left alone: nothing to fold into.
    let failed = DesktopResponse::err(
        "press",
        tinydesktop_bus::DesktopError::new("PERM_DENIED", "no automation permission"),
    );
    assert_eq!(with_restoration(failed.clone(), false), failed);
}

#[test]
fn an_app_with_several_windows_counts_as_launched() {
    let ambiguous = DesktopResponse::err(
        "launch",
        tinydesktop_bus::DesktopError::new("AMBIGUOUS_TARGET", "several windows"),
    );
    assert!(running_is_launched(ambiguous).ok);
    let missing = DesktopResponse::err(
        "launch",
        tinydesktop_bus::DesktopError::new("APP_NOT_FOUND", "no such app"),
    );
    assert!(!running_is_launched(missing).ok);
}

#[tokio::test]
async fn the_desktop_backend_fails_closed_on_empty_targets_without_touching_input() {
    // Every call names nothing, so each fails before pressing, pasting, or
    // launching anything on the machine running the tests.
    let desktop = crate::Desktop::new();
    let empty = Candidate::default();
    assert!(AgentBackend::read_value(&desktop, &empty).is_none());
    assert!(!AgentBackend::paste(&desktop, "", &empty, "text").ok);
    assert!(!AgentBackend::press(&desktop, "", "").ok);
    assert!(!AgentBackend::launch(&desktop, "").ok);
    assert!(AgentBackend::observe(&desktop, "__tinydesktop_missing__", None, Depth::Full).is_err());
    AgentBackend::settle(&TextBackend::default());
    let typed = blocking(desktop, move |desktop| {
        deliver_text(&desktop, "", &empty, "text")
    })
    .await;
    assert!(!typed.ok);
}

#[test]
fn every_closed_operation_dispatches_without_panicking() {
    let desktop = crate::Desktop::new();
    let candidate = Candidate::default();
    for operation in [
        JevOperation::Click,
        JevOperation::TypeText,
        JevOperation::Check,
        JevOperation::Uncheck,
        JevOperation::Expand,
        JevOperation::Collapse,
        JevOperation::Scroll,
        JevOperation::Wait,
        JevOperation::Drill,
        JevOperation::Widen,
        JevOperation::Done,
        JevOperation::Blocked,
    ] {
        let reply = execute_desktop(
            &desktop,
            operation,
            Some(&candidate),
            Some("text".to_owned()),
        );
        assert!(!reply.command.is_empty());
    }
}
