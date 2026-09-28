//! Tests for acting: launch replies, failing closed without touching input, the
//! cursor glide, and screen bounds.

use serde_json::json;
use tinycomputer_bus::{DesktopResponse, JevOperation};
use tinycomputer_core::surface::{Candidate, Depth, Surface, deliver_text};

use crate::surface::act::{execute_desktop, running_is_launched, screen_bounds};

#[test]
fn an_app_with_several_windows_counts_as_launched() {
    let ambiguous = DesktopResponse::err(
        "launch",
        tinycomputer_bus::DesktopError::new("AMBIGUOUS_TARGET", "several windows"),
    );
    assert!(running_is_launched(ambiguous).ok);
    let missing = DesktopResponse::err(
        "launch",
        tinycomputer_bus::DesktopError::new("APP_NOT_FOUND", "no such app"),
    );
    assert!(!running_is_launched(missing).ok);
}

#[test]
fn the_desktop_backend_fails_closed_on_empty_targets_without_touching_input() {
    // Every call names nothing, so each fails before pressing, pasting, or
    // launching anything on the machine running the tests.
    let desktop = crate::Desktop::new();
    let empty = Candidate::default();
    assert!(Surface::read_value(&desktop, &empty).is_none());
    assert!(!Surface::paste(&desktop, "", &empty, "text").ok);
    assert!(!Surface::press(&desktop, "", "").ok);
    assert!(!Surface::launch(&desktop, "").ok);
    assert!(Surface::observe(&desktop, "__tinycomputer_missing__", None, Depth::Full).is_err());
    assert!(!deliver_text(&desktop, "", &empty, "text").ok);
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

#[derive(Clone, Default)]
struct Drawn(std::sync::Arc<std::sync::Mutex<Vec<tinycomputer_cursor::OverlayCommand>>>);

impl tinycomputer_cursor::OverlaySink for Drawn {
    fn send(&mut self, command: &tinycomputer_cursor::OverlayCommand) -> std::io::Result<()> {
        self.0.lock().unwrap().push(command.clone());
        Ok(())
    }
}

fn boxed(bounds: serde_json::Value) -> Candidate {
    Candidate {
        ref_id: "@s:e1".to_owned(),
        role: "button".to_owned(),
        bounds: Some(bounds),
        ..Candidate::default()
    }
}

#[test]
fn a_candidates_screen_bounds_are_read_when_complete_and_positive() {
    let rect = screen_bounds(&boxed(
        json!({"x": 10.0, "y": 20.0, "width": 80.0, "height": 24.0}),
    ))
    .expect("complete bounds parse");
    assert!((rect.width - 80.0).abs() < f64::EPSILON && (rect.y - 20.0).abs() < f64::EPSILON);
    for broken in [
        json!({"x": 10.0, "y": 20.0}),
        json!({"x": 10.0, "y": 20.0, "width": 0.0, "height": 24.0}),
        json!("somewhere"),
    ] {
        assert!(screen_bounds(&boxed(broken)).is_none());
    }
    assert!(screen_bounds(&Candidate::default()).is_none());
}

#[test]
fn a_pointer_operation_glides_the_cursor_onto_its_target_first() {
    use tinycomputer_cursor::{CursorPace, OverlayCommand, ScreenCursor};
    let drawn = Drawn::default();
    let cursor =
        ScreenCursor::with_sink(CursorPace::Natural, Box::new(drawn.clone())).without_waiting();
    let desktop = crate::Desktop::new().with_cursor(std::sync::Arc::new(cursor));
    let target = boxed(json!({"x": 400.0, "y": 300.0, "width": 120.0, "height": 32.0}));

    // Headless and permission-less, the action itself fails closed as before;
    // the cursor has already glided onto where it would land.
    let _clicked = execute_desktop(&desktop, JevOperation::Click, Some(&target), None);
    let _scrolled = execute_desktop(&desktop, JevOperation::Scroll, Some(&target), None);
    let _unboxed = execute_desktop(
        &desktop,
        JevOperation::Check,
        Some(&Candidate::default()),
        None,
    );
    let sent = drawn.0.lock().unwrap();
    assert_eq!(sent.len(), 1, "only the boxed pointer operation glides");
    let OverlayCommand::Glide { path, .. } = &sent[0] else {
        panic!("a glide");
    };
    let [_, x, y] = *path.last().unwrap();
    assert!((400.0..=520.0).contains(&x) && (300.0..=332.0).contains(&y));
}
