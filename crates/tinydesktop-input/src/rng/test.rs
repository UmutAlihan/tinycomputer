//! Tests for the seeded generator.

use super::Rng;

#[test]
fn a_seed_reproduces_its_sequence() {
    let mut a = Rng::seeded(7);
    let mut b = Rng::seeded(7);
    for _ in 0..32 {
        assert_eq!(a.unit().to_bits(), b.unit().to_bits());
    }
    assert_ne!(Rng::seeded(7).unit().to_bits(), Rng::seeded(8).unit().to_bits());
}

#[test]
fn draws_stay_in_their_ranges() {
    let mut rng = Rng::seeded(1);
    for _ in 0..10_000 {
        let unit = rng.unit();
        assert!((0.0..1.0).contains(&unit));
        let ranged = rng.range(-3.0, 5.0);
        assert!((-3.0..5.0).contains(&ranged));
        assert!(rng.normal(0.0, 1.0).is_finite());
    }
}

#[test]
fn normal_draws_center_on_their_mean() {
    let mut rng = Rng::seeded(99);
    let count = 20_000_u32;
    let sum: f64 = (0..count).map(|_| rng.normal(10.0, 2.0)).sum();
    assert!((sum / f64::from(count) - 10.0).abs() < 0.1);
}

#[test]
fn chance_respects_its_probability() {
    let mut rng = Rng::seeded(3);
    assert!(!(0..100).any(|_| rng.chance(0.0)));
    assert!((0..100).all(|_| rng.chance(1.0)));
    let hits = (0..10_000).filter(|_| rng.chance(0.25)).count();
    assert!((2_200..2_800).contains(&hits), "{hits}");
}

#[test]
fn entropy_seeds_differ() {
    let a = Rng::from_entropy().unit();
    let b = Rng::default().unit();
    assert_ne!(a.to_bits(), b.to_bits());
}
