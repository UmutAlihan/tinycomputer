//! Tests for the overlay driver.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::mpsc;

use tinycomputer_cursor::OverlayCommand;

use super::{Driver, Tick};

#[test]
fn a_glide_places_the_window_so_the_tip_sits_on_the_point() {
    let (send, receive) = mpsc::channel();
    let mut driver = Driver::new(receive);
    assert_eq!(driver.tick_at(0.0), Tick::Hidden);
    send.send(OverlayCommand::Glide {
        path: vec![[0.0, 100.0, 200.0], [100.0, 300.0, 400.0]],
        appears: false,
    })
    .unwrap();
    let Tick::Show(start) = driver.tick_at(0.0) else {
        panic!("shown")
    };
    assert_eq!((start.origin.x, start.origin.y), (68.0, 168.0));
    assert_eq!(start.sprite_frame, 0);
    let Tick::Show(landed) = driver.tick_at(150.0) else {
        panic!("shown")
    };
    assert_eq!((landed.origin.x, landed.origin.y), (268.0, 368.0));
    assert!(landed.sprite_frame > 0, "pulsing on landing");
    assert!(matches!(driver.tick(), Tick::Show(_) | Tick::Hidden));
    drop(send);
    assert_eq!(driver.tick_at(200.0), Tick::Quit);
}
