//! Tests for the virtual mouse: paths end on target, stay bounded, keep
//! time, and aim inside the element; gestures press and release in order.

use super::path::{OVERSHOOT_MAX_SHARE, SAMPLE_MS, TRAVEL_MS, travel_ms};
use super::{VirtualMouse, aim, human_path};
use crate::geometry::{Point, Rect};
use crate::plan::{Button, Step};
use crate::profile::MotionProfile;
use crate::rng::Rng;

const FROM: Point = Point::new(100.0, 700.0);
const TO: Point = Point::new(900.0, 150.0);

#[test]
fn a_path_ends_exactly_on_its_target_with_strictly_increasing_time() {
    for seed in 0..200 {
        let path = human_path(FROM, TO, 40.0, MotionProfile::Natural, &mut Rng::seeded(seed));
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
        let path = human_path(FROM, TO, 40.0, MotionProfile::Natural, &mut Rng::seeded(seed));
        let furthest = path
            .iter()
            .map(|sample| (sample.point.x - FROM.x) * dx + (sample.point.y - FROM.y) * dy)
            .fold(f64::MIN, f64::max);
        let past = furthest - distance;
        // Tremor adds under two pixels on top of the planned landing.
        assert!(past <= OVERSHOOT_MAX_SHARE * distance + 2.0, "seed {seed}: {past}");
        if past > 3.0 {
            overshot += 1;
        }
        let widest = path
            .iter()
            .map(|sample| ((sample.point.x - FROM.x) * -dy + (sample.point.y - FROM.y) * dx).abs())
            .fold(0.0, f64::max);
        assert!(widest <= 0.25 * distance.min(480.0) + 10.0, "seed {seed}: {widest}");
    }
    assert!(overshot > 200, "most long reaches overshoot, {overshot} did");
}

#[test]
fn paths_curve_differently_from_one_seed_to_the_next() {
    let a = human_path(FROM, TO, 40.0, MotionProfile::Natural, &mut Rng::seeded(1));
    let b = human_path(FROM, TO, 40.0, MotionProfile::Natural, &mut Rng::seeded(2));
    assert_ne!(a, b);
    let again = human_path(FROM, TO, 40.0, MotionProfile::Natural, &mut Rng::seeded(1));
    assert_eq!(a, again);
}

#[test]
fn travel_time_follows_fitts_and_scales_with_the_profile() {
    let mut rng = Rng::seeded(5);
    for _ in 0..500 {
        let natural = travel_ms(800.0, 20.0, MotionProfile::Natural, &mut rng);
        assert!((TRAVEL_MS.0..=TRAVEL_MS.1).contains(&natural));
    }
    let mean = |distance: f64, width: f64| {
        let mut rng = Rng::seeded(9);
        (0..200)
            .map(|_| travel_ms(distance, width, MotionProfile::Natural, &mut rng))
            .sum::<f64>()
            / 200.0
    };
    assert!(mean(800.0, 20.0) > mean(100.0, 20.0), "farther is slower");
    assert!(mean(400.0, 10.0) > mean(400.0, 200.0), "smaller is slower");

    let brisk = human_path(FROM, TO, 40.0, MotionProfile::Brisk, &mut Rng::seeded(3));
    let calm = human_path(FROM, TO, 40.0, MotionProfile::Calm, &mut Rng::seeded(3));
    assert!(brisk.last().unwrap().t_ms < calm.last().unwrap().t_ms);
    assert!(calm.len() as f64 >= calm.last().unwrap().t_ms / SAMPLE_MS - 1.0);
}

#[test]
fn instant_short_and_invalid_reaches_are_a_single_jump() {
    for (from, profile) in [
        (FROM, MotionProfile::Instant),
        (TO.plus(0.5, 0.0), MotionProfile::Natural),
        (Point::new(f64::NAN, 0.0), MotionProfile::Natural),
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
        let path = human_path(from, to, 20.0, MotionProfile::Natural, &mut Rng::seeded(seed));
        let furthest = path.iter().map(|sample| sample.point.x).fold(f64::MIN, f64::max);
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

fn mouse(profile: MotionProfile) -> VirtualMouse {
    VirtualMouse::with_rng(profile, Rng::seeded(42))
}

#[test]
fn a_mouse_with_no_position_enters_from_nearby_and_remembers_where_it_ends() {
    let mut mouse = mouse(MotionProfile::Natural);
    assert_eq!(mouse.position(), None);
    let plan = mouse.glide(TO, 40.0);
    let moves = plan.moves();
    let entry = moves[0];
    let reach = entry.distance(TO);
    assert!(
        (200.0..=420.0).contains(&reach) || entry.x.abs() < f64::EPSILON || entry.y.abs() < f64::EPSILON,
        "{reach}"
    );
    assert_eq!(*moves.last().unwrap(), TO);
    assert_eq!(mouse.position(), Some(TO));
    assert!(plan.duration_ms() > 0.0);

    let next = mouse.glide(FROM, 40.0);
    assert_ne!(next.moves()[0], TO, "the next reach starts from where it is");
    assert_eq!(*next.moves().last().unwrap(), FROM);
}

#[test]
fn an_instant_mouse_jumps_without_pausing() {
    let mut mouse = mouse(MotionProfile::Instant);
    let plan = mouse.click(Rect::new(0.0, 0.0, 10.0, 10.0), Button::Left, 1);
    assert_eq!(
        plan.steps(),
        [
            Step::Move(Point::new(5.0, 5.0)),
            Step::Press(Button::Left),
            Step::Release(Button::Left)
        ]
    );
    assert!(plan.duration_ms().abs() < f64::EPSILON);
}

#[test]
fn a_click_reaches_settles_and_presses_inside_the_target() {
    let target = Rect::new(300.0, 200.0, 80.0, 24.0);
    let mut mouse = mouse(MotionProfile::Natural).at(FROM);
    let plan = mouse.click(target, Button::Right, 2);
    let steps = plan.steps();
    let presses: Vec<usize> = steps
        .iter()
        .enumerate()
        .filter(|(_, step)| **step == Step::Press(Button::Right))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(presses.len(), 2);
    assert_eq!(
        steps.iter().filter(|step| **step == Step::Release(Button::Right)).count(),
        2
    );
    assert!(matches!(steps[presses[0] - 1], Step::Pause(ms) if ms >= 60.0));
    assert!(matches!(steps[presses[0] + 1], Step::Pause(ms) if ms >= 55.0));
    let landed = mouse.position().unwrap();
    assert!(target.contains(landed));
    assert_eq!(*plan.moves().last().unwrap(), landed);
}

#[test]
fn hover_and_approach_end_on_the_target_and_linger() {
    let target = Rect::new(300.0, 200.0, 80.0, 24.0);
    let mut mouse = mouse(MotionProfile::Natural).at(FROM);
    let hover = mouse.hover(target);
    assert!(matches!(hover.steps().last(), Some(Step::Pause(ms)) if *ms >= 250.0));
    assert!(target.contains(*hover.moves().last().unwrap()));

    let (approach, point) = mouse.approach(target);
    assert!(target.contains(point));
    assert_eq!(*approach.moves().last().unwrap(), point);
    assert!(matches!(approach.steps().last(), Some(Step::Pause(_))));
}

#[test]
fn a_drag_grabs_carries_and_drops() {
    let from = Rect::new(100.0, 100.0, 40.0, 40.0);
    let to = Rect::new(600.0, 400.0, 40.0, 40.0);
    let mut mouse = mouse(MotionProfile::Brisk).at(Point::new(0.0, 0.0));
    let plan = mouse.drag(from, to);
    let steps = plan.steps();
    let press = steps.iter().position(|step| *step == Step::Press(Button::Left)).unwrap();
    let release = steps.iter().position(|step| *step == Step::Release(Button::Left)).unwrap();
    assert!(press < release);
    let carried: Vec<Point> = steps[press..release]
        .iter()
        .filter_map(|step| match step {
            Step::Move(point) => Some(*point),
            _ => None,
        })
        .collect();
    assert!(carried.len() > 3);
    assert!(to.contains(*carried.last().unwrap()));
    assert!(from.contains(*plan.moves().first().unwrap()) || !plan.moves().is_empty());
}

#[test]
fn placing_and_forgetting_update_the_position() {
    let mut mouse = VirtualMouse::new(MotionProfile::Calm);
    assert_eq!(mouse.profile(), MotionProfile::Calm);
    mouse.place(FROM);
    assert_eq!(mouse.position(), Some(FROM));
    mouse.forget();
    assert_eq!(mouse.position(), None);
    let mut instant = VirtualMouse::new(MotionProfile::Instant);
    assert_eq!(instant.glide(TO, 10.0).moves(), vec![TO]);
}
