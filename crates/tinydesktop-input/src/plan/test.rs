//! Tests for plans and playing them against a recording sink.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use super::{Button, InputSink, Key, Plan, Step, play};
use crate::geometry::Point;

/// Records every primitive; fails the one named in `fail_on`.
#[derive(Default)]
struct Recorder {
    log: Vec<String>,
    fail_on: Option<&'static str>,
}

impl Recorder {
    fn record(&mut self, entry: String, kind: &'static str) -> Result<(), String> {
        if self.fail_on == Some(kind) {
            return Err(format!("{kind} failed"));
        }
        self.log.push(entry);
        Ok(())
    }
}

impl InputSink for Recorder {
    type Error = String;

    fn move_to(&mut self, point: Point) -> Result<(), String> {
        self.record(format!("move {} {}", point.x, point.y), "move")
    }
    fn press(&mut self, button: Button) -> Result<(), String> {
        self.record(format!("press {}", button.as_str()), "press")
    }
    fn release(&mut self, button: Button) -> Result<(), String> {
        self.record(format!("release {}", button.as_str()), "release")
    }
    fn key_down(&mut self, key: Key) -> Result<(), String> {
        self.record(format!("down {}", key.name()), "down")
    }
    fn key_up(&mut self, key: Key) -> Result<(), String> {
        self.record(format!("up {}", key.name()), "up")
    }
    fn text(&mut self, text: &str) -> Result<(), String> {
        self.record(format!("text {text}"), "text")
    }
}

fn sample() -> Plan {
    let mut plan = Plan::new();
    plan.push(Step::Move(Point::new(1.0, 2.0)));
    plan.push(Step::Pause(10.0));
    plan.push(Step::Pause(5.0));
    plan.push(Step::Pause(0.0));
    plan.push(Step::Pause(f64::NAN));
    plan.push(Step::Press(Button::Right));
    plan.push(Step::Release(Button::Right));
    plan.push(Step::KeyDown(Key::Enter));
    plan.push(Step::KeyUp(Key::Enter));
    plan.push(Step::Text("hi".to_owned()));
    plan
}

#[test]
fn consecutive_pauses_merge_and_empty_ones_drop() {
    let plan = sample();
    assert_eq!(plan.steps()[1], Step::Pause(15.0));
    assert_eq!(plan.steps().len(), 7);
    assert!((plan.duration_ms() - 15.0).abs() < 1e-9);
    assert_eq!(plan.moves(), vec![Point::new(1.0, 2.0)]);
    assert!(!plan.is_empty());
    assert!(Plan::default().is_empty());
}

#[test]
fn playing_performs_every_primitive_in_order_and_waits_for_pauses() {
    let mut sink = Recorder::default();
    let mut waited = Vec::new();
    play(&sample(), &mut sink, &mut |duration| waited.push(duration)).unwrap();
    assert_eq!(
        sink.log,
        [
            "move 1 2",
            "press right",
            "release right",
            "down Enter",
            "up Enter",
            "text hi"
        ]
    );
    assert_eq!(waited, [Duration::from_millis(15)]);
}

#[test]
fn playing_stops_at_the_first_failure() {
    for kind in ["move", "press", "release", "down", "up", "text"] {
        let mut sink = Recorder {
            fail_on: Some(kind),
            ..Recorder::default()
        };
        let error = play(&sample(), &mut sink, &mut |_| {}).unwrap_err();
        assert_eq!(error, format!("{kind} failed"));
    }
}

#[test]
fn extending_merges_a_trailing_pause_with_a_leading_one() {
    let mut first = Plan::new();
    first.push(Step::Pause(4.0));
    let mut second = Plan::new();
    second.push(Step::Pause(6.0));
    second.push(Step::Press(Button::Left));
    first.extend(second);
    assert_eq!(
        first.steps(),
        [Step::Pause(10.0), Step::Press(Button::Left)]
    );
}

#[test]
fn keys_and_buttons_name_themselves() {
    assert_eq!(Key::Char('a').name(), "a");
    assert_eq!(Key::Char('a').text(), "a");
    assert_eq!(Key::Enter.name(), "Enter");
    assert_eq!(Key::Enter.text(), "\r");
    assert_eq!(Key::Tab.name(), "Tab");
    assert_eq!(Key::Tab.text(), "\t");
    assert_eq!(Button::default().as_str(), "left");
    assert_eq!(Button::Middle.as_str(), "middle");
}
