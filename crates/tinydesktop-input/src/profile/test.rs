//! Tests for motion profiles: names, parsing, and the wire form.

use super::MotionProfile;
use crate::Error;

#[test]
fn names_round_trip_through_parsing() {
    for profile in MotionProfile::ALL {
        assert_eq!(profile.as_str().parse::<MotionProfile>(), Ok(profile));
        assert_eq!(profile.to_string(), profile.as_str());
    }
    assert_eq!(" Calm ".parse::<MotionProfile>(), Ok(MotionProfile::Calm));
}

#[test]
fn an_unknown_name_is_refused() {
    assert_eq!(
        "frantic".parse::<MotionProfile>(),
        Err(Error::UnknownProfile {
            name: "frantic".to_owned()
        })
    );
}

#[test]
fn natural_is_the_default_and_only_instant_is_instant() {
    assert_eq!(MotionProfile::default(), MotionProfile::Natural);
    assert!(MotionProfile::Instant.is_instant());
    assert!(!MotionProfile::Brisk.is_instant());
    assert!(MotionProfile::Brisk.tempo() < MotionProfile::Natural.tempo());
    assert!(MotionProfile::Natural.tempo() < MotionProfile::Calm.tempo());
}

#[test]
fn the_wire_form_is_the_lowercase_name() {
    assert_eq!(
        serde_json::to_value(MotionProfile::Brisk).unwrap(),
        serde_json::json!("brisk")
    );
    assert_eq!(
        serde_json::from_value::<MotionProfile>(serde_json::json!("calm")).unwrap(),
        MotionProfile::Calm
    );
}
