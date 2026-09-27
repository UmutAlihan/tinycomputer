//! Tests for points and rectangles.

use super::{Point, Rect};

#[test]
fn distance_is_euclidean() {
    assert!((Point::new(0.0, 0.0).distance(Point::new(3.0, 4.0)) - 5.0).abs() < 1e-9);
}

#[test]
fn a_rectangle_knows_its_center_and_what_it_contains() {
    let rect = Rect::new(10.0, 20.0, 100.0, 40.0);
    assert_eq!(rect.center(), Point::new(60.0, 40.0));
    assert!(rect.contains(Point::new(10.0, 20.0)));
    assert!(rect.contains(Point::new(110.0, 60.0)));
    assert!(!rect.contains(Point::new(111.0, 40.0)));
    assert_eq!(Rect::at(Point::new(4.0, 5.0)).center(), Point::new(4.0, 5.0));
}

#[test]
fn validity_rejects_negative_sides_and_non_finite_fields() {
    assert!(Rect::new(0.0, 0.0, 1.0, 1.0).is_valid());
    assert!(!Rect::new(0.0, 0.0, -1.0, 1.0).is_valid());
    assert!(!Rect::new(f64::NAN, 0.0, 1.0, 1.0).is_valid());
    assert!(Point::new(1.0, 2.0).is_finite());
    assert!(!Point::new(f64::INFINITY, 2.0).is_finite());
}

#[test]
fn lerp_and_plus_move_a_point() {
    let a = Point::new(0.0, 0.0);
    assert_eq!(a.lerp(Point::new(10.0, 20.0), 0.5), Point::new(5.0, 10.0));
    assert_eq!(a.plus(1.0, -1.0), Point::new(1.0, -1.0));
}
