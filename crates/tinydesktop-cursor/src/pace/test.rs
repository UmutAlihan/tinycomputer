//! Tests for cursor paces: names, parsing, and the wire form.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::CursorPace;
use crate::Error;

#[test]
fn names_round_trip_through_parsing() {
    for pace in CursorPace::ALL {
        assert_eq!(pace.as_str().parse::<CursorPace>(), Ok(pace));
        assert_eq!(pace.to_string(), pace.as_str());
    }
    assert_eq!(" Calm ".parse::<CursorPace>(), Ok(CursorPace::Calm));
}

#[test]
fn an_unknown_name_is_refused() {
    assert_eq!(
        "frantic".parse::<CursorPace>(),
        Err(Error::UnknownPace {
            name: "frantic".to_owned()
        })
    );
}

#[test]
fn natural_is_the_default_and_only_off_is_off() {
    assert_eq!(CursorPace::default(), CursorPace::Natural);
    assert!(CursorPace::Off.is_off());
    assert!(!CursorPace::Brisk.is_off());
    assert!(CursorPace::Brisk.tempo() < CursorPace::Natural.tempo());
    assert!(CursorPace::Natural.tempo() < CursorPace::Calm.tempo());
}

#[test]
fn the_wire_form_is_the_lowercase_name() {
    assert_eq!(
        serde_json::to_value(CursorPace::Brisk).unwrap(),
        serde_json::json!("brisk")
    );
    assert_eq!(
        serde_json::from_value::<CursorPace>(serde_json::json!("off")).unwrap(),
        CursorPace::Off
    );
}
