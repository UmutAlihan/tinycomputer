//! Tests for the virtual keyboard: typed text is reproduced exactly, one
//! key at a time, with gaps inside the tempo's band.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::VirtualKeyboard;
use super::cadence::{GAP_BOUNDS, HOLD_MS, MEDIAN_GAP_MS, THINK_MS, gap_ms, hold_ms};
use crate::plan::{Key, Step};
use crate::profile::MotionProfile;
use crate::rng::Rng;

fn keyboard(profile: MotionProfile) -> VirtualKeyboard {
    VirtualKeyboard::with_rng(profile, Rng::seeded(7))
}

fn typed(steps: &[Step]) -> String {
    steps
        .iter()
        .filter_map(|step| match step {
            Step::KeyDown(key) => Some(key.text()),
            _ => None,
        })
        .collect()
}

#[test]
fn every_character_is_pressed_held_and_released_in_order() {
    let text = "Hello, Sam!\nSee you Friday.\tBye";
    let plan = keyboard(MotionProfile::Natural).type_text(text);
    assert_eq!(typed(plan.steps()), text.replace('\n', "\r"));
    let downs = plan
        .steps()
        .iter()
        .filter(|step| matches!(step, Step::KeyDown(_)))
        .count();
    let ups = plan
        .steps()
        .iter()
        .filter(|step| matches!(step, Step::KeyUp(_)))
        .count();
    assert_eq!((downs, ups), (text.chars().count(), text.chars().count()));
    assert!(plan.steps().contains(&Step::KeyDown(Key::Enter)));
    assert!(plan.steps().contains(&Step::KeyDown(Key::Tab)));
    for window in plan.steps().windows(3) {
        if let [Step::KeyDown(down), Step::Pause(_), Step::KeyUp(up)] = window {
            assert_eq!(down, up);
        }
    }
}

#[test]
fn carriage_returns_fold_into_one_enter() {
    let plan = keyboard(MotionProfile::Brisk).type_text("a\r\nb");
    assert_eq!(typed(plan.steps()), "a\rb");
}

#[test]
fn the_instant_profile_inserts_at_once_and_empty_text_is_a_no_op() {
    let plan = keyboard(MotionProfile::Instant).type_text("fast");
    assert_eq!(plan.steps(), [Step::Text("fast".to_owned())]);
    assert!(keyboard(MotionProfile::Natural).type_text("").is_empty());
    assert_eq!(
        VirtualKeyboard::new(MotionProfile::Calm).profile(),
        MotionProfile::Calm
    );
}

#[test]
fn gaps_and_holds_stay_inside_the_tempo_band() {
    let mut rng = Rng::seeded(3);
    let pairs = [('a', 'b'), ('t', 'h'), (' ', 'W'), ('.', ' '), ('x', 'y')];
    for _ in 0..2_000 {
        for (previous, next) in pairs {
            let gap = gap_ms(previous, next, MotionProfile::Natural, &mut rng);
            let floor = MEDIAN_GAP_MS * GAP_BOUNDS.0;
            let ceiling = MEDIAN_GAP_MS * GAP_BOUNDS.1 + THINK_MS.1;
            assert!((floor..=ceiling).contains(&gap), "{previous}{next}: {gap}");
        }
        let hold = hold_ms(MotionProfile::Natural, &mut rng);
        assert!((HOLD_MS.0..=HOLD_MS.1).contains(&hold));
    }
}

#[test]
fn common_pairs_are_faster_and_punctuation_slower_on_average() {
    let mean = |previous: char, next: char| {
        let mut rng = Rng::seeded(17);
        (0..4_000)
            .map(|_| gap_ms(previous, next, MotionProfile::Natural, &mut rng))
            .sum::<f64>()
            / 4_000.0
    };
    assert!(mean('t', 'h') < mean('q', 'z'));
    assert!(mean('.', 'a') > mean('q', 'z'));
    assert!(mean('q', 'Z') > mean('q', 'z'));
}

#[test]
fn a_faster_profile_types_faster() {
    let text = "the quick brown fox jumps over the lazy dog";
    let brisk = keyboard(MotionProfile::Brisk).type_text(text).duration_ms();
    let calm = keyboard(MotionProfile::Calm).type_text(text).duration_ms();
    assert!(brisk < calm);
    let natural = keyboard(MotionProfile::Natural)
        .type_text(text)
        .duration_ms();
    let per_key = natural / 43.0;
    assert!((100.0..300.0).contains(&per_key), "{per_key}");
}
