//! Tests for the overlay helper's command reader and driver wiring.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Cursor;

use crate::{forward_lines, run};

#[test]
fn the_reader_starts_and_the_driver_is_launched() {
    run(Cursor::new(Vec::new()), drop);
}

#[test]
fn valid_lines_are_forwarded_and_malformed_lines_are_ignored() {
    let (sender, receiver) = std::sync::mpsc::channel();
    let command = tinycomputer_cursor::OverlayCommand::Glide {
        path: vec![[0.0, 10.0, 20.0]],
        appears: true,
    };
    let input = Cursor::new(format!("not a command\n{}", command.to_line()));

    forward_lines(input, &sender);

    assert_eq!(receiver.try_recv(), Ok(command));
    assert!(receiver.try_recv().is_err());
}

#[test]
fn the_reader_stops_when_the_driver_has_gone() {
    let (sender, receiver) = std::sync::mpsc::channel();
    drop(receiver);
    let input = Cursor::new(tinycomputer_cursor::OverlayCommand::Hide.to_line());

    forward_lines(input, &sender);
}
