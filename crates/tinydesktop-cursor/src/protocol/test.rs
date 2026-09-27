//! Tests for the overlay protocol's wire form.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::OverlayCommand;
use crate::{CursorPace, Point, Rect, Rng, VirtualCursor};

#[test]
fn a_glide_starts_at_its_origin_and_rounds_its_positions() {
    let glide = VirtualCursor::with_rng(CursorPace::Natural, Rng::seeded(3))
        .at(Point::new(10.04, 20.06))
        .glide(Rect::new(400.0, 300.0, 120.0, 32.0))
        .unwrap();
    let OverlayCommand::Glide { path, appears } = OverlayCommand::glide(&glide) else {
        panic!("a glide command");
    };
    assert!(!appears);
    assert_eq!(path[0], [0.0, 10.0, 20.1]);
    assert_eq!(path.len(), glide.samples.len() + 1);
    let last = path.last().unwrap();
    assert!((last[1] - glide.to.x).abs() <= 0.05 && (last[2] - glide.to.y).abs() <= 0.05);
}

#[test]
fn commands_travel_as_one_tagged_json_object_per_line() {
    let glide = OverlayCommand::Glide {
        path: vec![[0.0, 1.0, 2.0], [16.0, 3.0, 4.0]],
        appears: true,
    };
    let line = glide.to_line();
    assert_eq!(
        line,
        "{\"type\":\"glide\",\"path\":[[0.0,1.0,2.0],[16.0,3.0,4.0]],\"appears\":true}\n"
    );
    assert_eq!(OverlayCommand::from_line(&line), Some(glide));
    assert_eq!(OverlayCommand::Hide.to_line(), "{\"type\":\"hide\"}\n");
    assert_eq!(OverlayCommand::from_line(" {\"type\":\"hide\"} "), Some(OverlayCommand::Hide));
}

#[test]
fn anything_else_is_ignored() {
    for line in ["", "hello", "{\"type\":\"dance\"}", "{\"type\":\"glide\"}"] {
        assert_eq!(OverlayCommand::from_line(line), None, "{line}");
    }
}
