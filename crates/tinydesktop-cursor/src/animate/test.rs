//! Tests for the animator: following a path, fading, pulsing, and hiding.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{Animator, FADE_MS, IDLE_MS, PULSE_MS};
use crate::geometry::Point;
use crate::protocol::OverlayCommand;

fn glide(appears: bool) -> OverlayCommand {
    OverlayCommand::Glide {
        path: vec![[0.0, 0.0, 0.0], [100.0, 50.0, 50.0], [200.0, 100.0, 100.0]],
        appears,
    }
}

#[test]
fn nothing_is_drawn_before_the_first_glide() {
    let animator = Animator::new();
    assert!(animator.frame(0.0).is_none());
    assert!(!animator.is_moving(0.0));
    assert!(animator.next_change(0.0).is_none());
}

#[test]
fn a_glide_follows_its_path_and_lands_exactly() {
    let mut animator = Animator::new();
    animator.apply(glide(false), 1_000.0);
    assert_eq!(
        animator.frame(1_000.0).unwrap().position,
        Point::new(0.0, 0.0)
    );
    assert_eq!(
        animator.frame(1_050.0).unwrap().position,
        Point::new(25.0, 25.0)
    );
    assert_eq!(
        animator.frame(1_150.0).unwrap().position,
        Point::new(75.0, 75.0)
    );
    assert_eq!(
        animator.frame(1_900.0).unwrap().position,
        Point::new(100.0, 100.0)
    );
    assert!(animator.is_moving(1_100.0));
}

#[test]
fn an_appearing_cursor_fades_in_and_a_landing_pulses_once() {
    let mut animator = Animator::new();
    animator.apply(glide(true), 0.0);
    assert!(animator.frame(0.0).unwrap().opacity.abs() < f64::EPSILON);
    assert!((animator.frame(FADE_MS / 2.0).unwrap().opacity - 0.5).abs() < 1e-9);
    assert!(animator.frame(199.0).unwrap().pulse.is_none());
    let pulse = animator
        .frame(200.0 + PULSE_MS / 2.0)
        .unwrap()
        .pulse
        .unwrap();
    assert!((pulse - 0.5).abs() < 1e-9);
    assert!(animator.frame(200.0 + PULSE_MS).unwrap().pulse.is_none());
    assert!(
        !animator.is_moving(200.0 + PULSE_MS + 1.0),
        "at rest after the pulse"
    );
}

#[test]
fn a_resting_cursor_fades_out_after_going_idle() {
    let mut animator = Animator::new();
    animator.apply(glide(false), 0.0);
    let idle = 200.0 + IDLE_MS;
    assert_eq!(animator.next_change(1_000.0), Some(idle));
    assert!((animator.frame(idle).unwrap().opacity - 1.0).abs() < 1e-9);
    assert!(animator.is_moving(idle + 1.0));
    assert!(animator.frame(idle + FADE_MS).is_none());
    assert!(animator.next_change(idle + FADE_MS).is_none());
}

#[test]
fn hide_fades_out_and_a_new_glide_fades_back_in() {
    let mut animator = Animator::new();
    animator.apply(OverlayCommand::Hide, 0.0);
    assert!(animator.frame(0.0).is_none(), "hiding nothing is a no-op");

    animator.apply(glide(false), 0.0);
    animator.apply(OverlayCommand::Hide, 500.0);
    animator.apply(OverlayCommand::Hide, 550.0);
    assert!((animator.frame(500.0 + FADE_MS / 2.0).unwrap().opacity - 0.5).abs() < 1e-9);
    assert!(animator.frame(500.0 + FADE_MS).is_none());

    animator.apply(glide(false), 1_000.0);
    let frame = animator.frame(1_000.0).unwrap();
    assert!(
        frame.opacity.abs() < f64::EPSILON,
        "a hidden cursor fades back in"
    );
}

#[test]
fn an_empty_glide_changes_nothing() {
    let mut animator = Animator::new();
    animator.apply(glide(false), 0.0);
    animator.apply(
        OverlayCommand::Glide {
            path: Vec::new(),
            appears: true,
        },
        50.0,
    );
    assert_eq!(
        animator.frame(50.0).unwrap().position,
        Point::new(25.0, 25.0)
    );
}

#[test]
fn a_path_with_repeated_times_still_moves_forward() {
    let mut animator = Animator::new();
    animator.apply(
        OverlayCommand::Glide {
            path: vec![[0.0, 0.0, 0.0], [0.0, 10.0, 10.0], [100.0, 20.0, 20.0]],
            appears: false,
        },
        0.0,
    );
    assert_eq!(
        animator.frame(50.0).unwrap().position,
        Point::new(15.0, 15.0)
    );
    assert_eq!(animator.frame(-5.0).unwrap().position, Point::new(0.0, 0.0));
}
