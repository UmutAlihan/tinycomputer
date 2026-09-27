//! Tests for the shared screen cursor: glides reach the sink, one cursor
//! continues across targets, and a missing or failing sink costs nothing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use super::{OverlaySink, ScreenCursor};
use crate::geometry::Rect;
use crate::pace::CursorPace;
use crate::protocol::OverlayCommand;

#[derive(Clone, Default)]
struct Recorder {
    sent: Arc<Mutex<Vec<OverlayCommand>>>,
    fail: bool,
}

impl OverlaySink for Recorder {
    fn send(&mut self, command: &OverlayCommand) -> std::io::Result<()> {
        if self.fail {
            return Err(std::io::Error::other("gone"));
        }
        self.sent.lock().unwrap().push(command.clone());
        Ok(())
    }
}

const MAIL_BUTTON: Rect = Rect::new(40.0, 700.0, 80.0, 24.0);
const WEB_BUTTON: Rect = Rect::new(900.0, 300.0, 120.0, 32.0);

fn path(command: &OverlayCommand) -> (&[[f64; 3]], bool) {
    match command {
        OverlayCommand::Glide { path, appears } => (path, *appears),
        OverlayCommand::Hide => panic!("expected a glide"),
    }
}

#[test]
fn one_cursor_glides_from_target_to_target_across_surfaces() {
    let recorder = Recorder::default();
    let cursor = ScreenCursor::with_sink(CursorPace::Natural, Box::new(recorder.clone()));
    assert_eq!(cursor.pace(), CursorPace::Natural);
    let cursor = cursor.without_waiting();
    cursor.show(MAIL_BUTTON);
    cursor.show(WEB_BUTTON);
    let sent = recorder.sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    let (first, appears) = path(&sent[0]);
    assert!(appears);
    let landed = *first.last().unwrap();
    assert!(MAIL_BUTTON.contains(crate::Point::new(landed[1], landed[2])));
    let (second, appears) = path(&sent[1]);
    assert!(!appears);
    assert_eq!(
        second[0].map(f64::to_bits),
        [0.0, landed[1], landed[2]].map(f64::to_bits),
        "no jump between surfaces"
    );
}

#[test]
fn hiding_fades_out_once_and_the_next_glide_fades_in() {
    let recorder = Recorder::default();
    let cursor =
        ScreenCursor::with_sink(CursorPace::Brisk, Box::new(recorder.clone())).without_waiting();
    cursor.hide();
    assert!(
        recorder.sent.lock().unwrap().is_empty(),
        "nothing on screen to hide"
    );
    cursor.show(MAIL_BUTTON);
    cursor.hide();
    cursor.hide();
    cursor.show(WEB_BUTTON);
    let sent = recorder.sent.lock().unwrap();
    assert_eq!(sent.len(), 3);
    assert_eq!(sent[1], OverlayCommand::Hide);
    assert!(path(&sent[2]).1);
}

#[test]
fn an_off_cursor_or_an_unusable_target_sends_nothing() {
    let recorder = Recorder::default();
    let off = ScreenCursor::with_sink(CursorPace::Off, Box::new(recorder.clone()));
    off.show(MAIL_BUTTON);
    off.hide();
    let on = ScreenCursor::with_sink(CursorPace::Natural, Box::new(recorder.clone()));
    on.show(Rect::new(0.0, 0.0, 0.0, 10.0));
    on.show(Rect::new(f64::NAN, 0.0, 10.0, 10.0));
    assert!(recorder.sent.lock().unwrap().is_empty());
    assert!(ScreenCursor::off().pace().is_off());
    ScreenCursor::off().show(MAIL_BUTTON);
}

#[test]
fn a_failing_sink_is_dropped_and_never_waited_on() {
    fn never(_: std::time::Duration) {
        panic!("a cursor nobody draws must not slow the action down");
    }
    let mut cursor = ScreenCursor::with_sink(
        CursorPace::Calm,
        Box::new(Recorder {
            fail: true,
            ..Recorder::default()
        }),
    );
    cursor.wait = never;
    cursor.show(MAIL_BUTTON);
    cursor.show(WEB_BUTTON);
    cursor.hide();
    assert!(format!("{cursor:?}").contains("Calm"));
}

#[test]
fn a_missing_helper_leaves_the_cursor_off_without_waiting() {
    fn never(_: std::time::Duration) {
        panic!("no helper, no wait");
    }
    let mut cursor = ScreenCursor::new(
        CursorPace::Natural,
        Some("/nonexistent/tinydesktop-cursor-overlay".into()),
    );
    cursor.wait = never;
    cursor.show(MAIL_BUTTON);
    cursor.show(WEB_BUTTON);
}

#[cfg(unix)]
#[test]
fn the_helper_process_receives_one_line_per_command() {
    use super::ProcessOverlay;
    let mut overlay = ProcessOverlay::spawn(Some(std::path::Path::new("/bin/cat"))).unwrap();
    overlay.send(&OverlayCommand::Hide).unwrap();
    drop(overlay);
    let _located = ProcessOverlay::locate();
}
