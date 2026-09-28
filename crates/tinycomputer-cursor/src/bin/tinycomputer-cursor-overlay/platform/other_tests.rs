//! Tests for the platform fallback that waits without drawing an overlay.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::mpsc;
use std::time::Duration;

use tinycomputer_cursor::OverlayCommand;

use crate::driver::Driver;

use super::{run, run_with_sleep};

#[test]
fn the_fallback_waits_for_commands_then_exits_when_the_sender_closes() {
    let (_, disconnected) = mpsc::channel();
    run(Driver::new(disconnected));

    let (sender, receiver) = mpsc::channel();
    sender.send(OverlayCommand::Hide).unwrap();
    let mut sender = Some(sender);

    run_with_sleep(
        &mut Driver::new(receiver),
        |_| drop(sender.take()),
        Duration::ZERO,
    );
}
