//! Tests for the surface-neutral helpers: fingerprints, change notes, names,
//! and verified text delivery over a scripted surface.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use serde_json::json;
use tinydesktop_bus::{DesktopResponse, JevOperation};

use super::{
    Candidate, Depth, Screen, Surface, change_note, deliver_text, exact_named_match, fingerprint,
    holds, target_payload, tokenized,
};

fn clickable_screen() -> Screen {
    Screen {
        app: "Spotify".to_owned(),
        window: Some("Liked Songs".to_owned()),
        surface: "window".to_owned(),
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
        candidates: vec![Candidate {
            ref_id: "@s1:e1".to_owned(),
            role: "button".to_owned(),
            name: Some("Play First Song by Artist".to_owned()),
            available_actions: vec!["Click".to_owned()],
            bounds: Some(json!({"x": 10.0, "y": 100.0})),
            ..Candidate::default()
        }],
    }
}

fn two_candidate_screen() -> Screen {
    let mut screen = clickable_screen();
    screen.candidates.push(Candidate {
        ref_id: "@s1:e2".to_owned(),
        role: "button".to_owned(),
        name: Some("Play Second Song by Artist".to_owned()),
        available_actions: vec!["Click".to_owned()],
        bounds: Some(json!({"x": 10.0, "y": 160.0})),
        ..Candidate::default()
    });
    screen
}

#[test]
fn a_fingerprint_ignores_ref_churn_between_snapshots() {
    let first = clickable_screen();
    let mut second = clickable_screen();
    second.candidates[0].ref_id = "@s2:e9".to_owned();
    assert_eq!(fingerprint(&first), fingerprint(&second));
    second.candidates[0].states = vec!["selected".to_owned()];
    assert_ne!(fingerprint(&first), fingerprint(&second));
}

#[test]
fn a_change_note_names_what_appeared_and_what_went_away() {
    let before = clickable_screen();
    let mut after = two_candidate_screen();
    after.candidates.remove(0);
    after.window = Some("Other".to_owned());
    after.surface = "sheet".to_owned();
    after.context = (0..8).map(|index| format!("line {index}")).collect();
    let note = change_note(&before, &after, true);
    assert!(note.contains("window is now \"Other\""), "{note}");
    assert!(note.contains("surface is now sheet"), "{note}");
    assert!(note.contains("and 3 more"), "{note}");
    assert!(note.contains("gone: button \"Play First Song"), "{note}");
    assert_eq!(
        change_note(&before, &before, false),
        "nothing on screen changed"
    );
    let mut same_labels = clickable_screen();
    same_labels.candidates[0].states = vec!["selected".to_owned()];
    assert_eq!(
        change_note(&before, &same_labels, true),
        "the screen changed"
    );
}

#[test]
fn exact_names_and_target_payloads_are_recognised() {
    let liked = Candidate {
        name: Some("Liked Songs".to_owned()),
        ..Candidate::default()
    };
    assert!(exact_named_match("open Liked Songs now", Some(&liked)));
    assert!(!exact_named_match("open Disliked Songs", Some(&liked)));
    assert!(!exact_named_match("open it", None));
    let single = Candidate {
        name: Some("Play".to_owned()),
        ..Candidate::default()
    };
    assert!(
        !exact_named_match("press Play", Some(&single)),
        "one word is not identity"
    );
    let described = target_payload(&Candidate {
        ref_id: "@s:e1".to_owned(),
        role: "button".to_owned(),
        description: Some("described".to_owned()),
        ..Candidate::default()
    });
    assert_eq!(described.name.as_deref(), Some("described"));
}

/// A backend whose reads, set-values, and pastes are scripted.
#[derive(Clone, Default)]
struct TextBackend {
    fail_execute: bool,
    fail_paste: bool,
    /// Values `read_value` returns, in order; exhausted means unreadable.
    reads: Arc<Mutex<VecDeque<String>>>,
    pastes: Arc<Mutex<Vec<String>>>,
}

impl Surface for TextBackend {
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
fn a_surface_settles_instantly_and_has_no_addresses_unless_it_says_otherwise() {
    Surface::settle(&TextBackend::default());
    let refused = Surface::navigate(&TextBackend::default(), "https://example.com");
    assert_eq!(refused.error.unwrap().code, "ACTION_NOT_SUPPORTED");
}
