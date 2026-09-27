//! Tests for the cursor: paths end on target, stay bounded, keep time, and
//! aim inside the element; glides start where the last one ended.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::path::{OVERSHOOT_MAX_SHARE, SAMPLE_MS, TRAVEL_MS, travel_ms};
use super::{VirtualCursor, aim, human_path};
use crate::geometry::{Point, Rect};
use crate::pace::CursorPace;
use crate::rng::Rng;

const FROM: Point = Point::new(100.0, 700.0);
const TO: Point = Point::new(900.0, 150.0);

#[test]
fn a_path_ends_exactly_on_its_target_with_strictly_increasing_time() {
    for seed in 0..200 {
        let path = human_path(FROM, TO, 40.0, CursorPace::Natural, &mut Rng::seeded(seed));
        assert_eq!(path.last().unwrap().point, TO, "seed {seed}");
        assert!(path.windows(2).all(|pair| pair[1].t_ms > pair[0].t_ms));
        assert!(path.len() > 5, "a long reach is sampled many times");
        assert!(path.iter().all(|sample| sample.point.is_finite()));
    }
}

#[test]
fn a_path_overshoots_by_at_most_its_bound_and_never_strays_far_sideways() {
    let distance = FROM.distance(TO);
    let (dx, dy) = ((TO.x - FROM.x) / distance, (TO.y - FROM.y) / distance);
    let mut overshot = 0;
    for seed in 0..500 {
        let path = human_path(FROM, TO, 40.0, CursorPace::Natural, &mut Rng::seeded(seed));
        let furthest = path
            .iter()
            .map(|sample| (sample.point.x - FROM.x) * dx + (sample.point.y - FROM.y) * dy)
            .fold(f64::MIN, f64::max);
        let past = furthest - distance;
        // Tremor adds under two pixels on top of the planned landing.
        assert!(
            past <= OVERSHOOT_MAX_SHARE * distance + 2.0,
            "seed {seed}: {past}"
        );
        if past > 3.0 {
            overshot += 1;
        }
        let widest = path
            .iter()
            .map(|sample| ((sample.point.x - FROM.x) * -dy + (sample.point.y - FROM.y) * dx).abs())
            .fold(0.0, f64::max);
        assert!(
            widest <= 0.25 * distance.min(480.0) + 10.0,
            "seed {seed}: {widest}"
        );
    }
    assert!(
        overshot > 200,
        "most long reaches overshoot, {overshot} did"
    );
}

#[test]
fn paths_curve_differently_from_one_seed_to_the_next() {
    let a = human_path(FROM, TO, 40.0, CursorPace::Natural, &mut Rng::seeded(1));
    let b = human_path(FROM, TO, 40.0, CursorPace::Natural, &mut Rng::seeded(2));
    assert_ne!(a, b);
    let again = human_path(FROM, TO, 40.0, CursorPace::Natural, &mut Rng::seeded(1));
    assert_eq!(a, again);
}

#[test]
fn travel_time_follows_fitts_and_scales_with_the_pace() {
    let mut rng = Rng::seeded(5);
    for _ in 0..500 {
        let natural = travel_ms(800.0, 20.0, CursorPace::Natural, &mut rng);
        assert!((TRAVEL_MS.0..=TRAVEL_MS.1).contains(&natural));
    }
    let mean = |distance: f64, width: f64| {
        let mut rng = Rng::seeded(9);
        (0..200)
            .map(|_| travel_ms(distance, width, CursorPace::Natural, &mut rng))
            .sum::<f64>()
            / 200.0
    };
    assert!(mean(800.0, 20.0) > mean(100.0, 20.0), "farther is slower");
    assert!(mean(400.0, 10.0) > mean(400.0, 200.0), "smaller is slower");

    let brisk = human_path(FROM, TO, 40.0, CursorPace::Brisk, &mut Rng::seeded(3));
    let calm = human_path(FROM, TO, 40.0, CursorPace::Calm, &mut Rng::seeded(3));
    assert!(brisk.last().unwrap().t_ms < calm.last().unwrap().t_ms);
    let expected = calm.last().unwrap().t_ms / SAMPLE_MS - 1.0;
    assert!(f64::from(u32::try_from(calm.len()).unwrap()) >= expected);
}

#[test]
fn off_short_and_invalid_reaches_are_a_single_jump() {
    for (from, profile) in [
        (FROM, CursorPace::Off),
        (TO.plus(0.5, 0.0), CursorPace::Natural),
        (Point::new(f64::NAN, 0.0), CursorPace::Natural),
    ] {
        let path = human_path(from, TO, 40.0, profile, &mut Rng::seeded(0));
        assert_eq!(path.len(), 1);
        assert_eq!(path[0].point, TO);
        assert!(path[0].t_ms.abs() < f64::EPSILON);
    }
}

#[test]
fn a_short_reach_does_not_overshoot() {
    let from = Point::new(100.0, 100.0);
    let to = Point::new(140.0, 100.0);
    for seed in 0..100 {
        let path = human_path(from, to, 20.0, CursorPace::Natural, &mut Rng::seeded(seed));
        let furthest = path
            .iter()
            .map(|sample| sample.point.x)
            .fold(f64::MIN, f64::max);
        assert!(furthest <= to.x + 2.0, "seed {seed}: {furthest}");
    }
}

#[test]
fn aim_always_lands_inside_the_middle_of_the_element() {
    let target = Rect::new(10.0, 20.0, 120.0, 30.0);
    let mut rng = Rng::seeded(11);
    let mut distinct = std::collections::HashSet::new();
    for _ in 0..5_000 {
        let point = aim(target, &mut rng);
        assert!(point.x >= 10.0 + 0.2 * 120.0 && point.x <= 10.0 + 0.8 * 120.0);
        assert!(point.y >= 20.0 + 0.25 * 30.0 && point.y <= 20.0 + 0.75 * 30.0);
        distinct.insert((point.x.to_bits(), point.y.to_bits()));
    }
    assert!(distinct.len() > 4_000, "aim varies");
}

#[test]
fn tiny_and_invalid_elements_are_aimed_at_their_centre() {
    let mut rng = Rng::seeded(0);
    let dot = Rect::new(5.0, 5.0, 1.0, 1.0);
    assert_eq!(aim(dot, &mut rng), dot.center());
    let broken = Rect::new(5.0, 5.0, -4.0, 10.0);
    assert_eq!(aim(broken, &mut rng), broken.center());
}

fn cursor(pace: CursorPace) -> VirtualCursor {
    VirtualCursor::with_rng(pace, Rng::seeded(42))
}

const BUTTON: Rect = Rect::new(300.0, 200.0, 80.0, 24.0);

#[test]
fn a_cursor_with_no_position_appears_nearby_and_remembers_where_it_lands() {
    let mut cursor = cursor(CursorPace::Natural);
    assert_eq!(cursor.position(), None);
    let glide = cursor.glide(BUTTON).unwrap();
    assert!(glide.appears);
    let reach = glide.from.distance(glide.to);
    assert!(
        (200.0..=420.0).contains(&reach) || glide.from.x == 0.0 || glide.from.y == 0.0,
        "{reach}"
    );
    assert!(BUTTON.contains(glide.to));
    assert_eq!(glide.samples.last().unwrap().point, glide.to);
    assert_eq!(cursor.position(), Some(glide.to));
    assert!(glide.duration_ms() > 100.0);

    let next = cursor.glide(Rect::new(40.0, 600.0, 60.0, 30.0)).unwrap();
    assert!(!next.appears);
    assert_eq!(
        next.from, glide.to,
        "the next glide starts where this one landed"
    );
}

#[test]
fn an_off_cursor_plans_nothing() {
    let mut off = cursor(CursorPace::Off);
    assert!(off.glide(BUTTON).is_none());
    assert_eq!(off.position(), None);
    assert_eq!(off.pace(), CursorPace::Off);
}

#[test]
fn a_calm_cursor_glides_slower_than_a_brisk_one() {
    let from = Point::new(900.0, 700.0);
    let brisk = cursor(CursorPace::Brisk).at(from).glide(BUTTON).unwrap();
    let calm = cursor(CursorPace::Calm).at(from).glide(BUTTON).unwrap();
    assert!(brisk.duration_ms() < calm.duration_ms());
    assert_eq!(brisk.from, from);
}

#[test]
fn placing_and_forgetting_update_the_position() {
    let mut cursor = VirtualCursor::new(CursorPace::Calm);
    cursor.place(FROM);
    assert_eq!(cursor.position(), Some(FROM));
    cursor.forget();
    assert_eq!(cursor.position(), None);
    assert!(cursor.glide(BUTTON).unwrap().appears);
}

#[test]
fn an_empty_glide_lasts_no_time() {
    let glide = super::Glide {
        from: FROM,
        to: FROM,
        samples: Vec::new(),
        appears: false,
    };
    assert!(glide.duration_ms().abs() < f64::EPSILON);
}
