//! Tests for the crate error's message.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::Error;

#[test]
fn an_unknown_profile_names_itself_and_the_choices() {
    let error = Error::UnknownProfile {
        name: "frantic".to_owned(),
    };
    assert_eq!(
        error.to_string(),
        "unknown motion profile `frantic`, expected instant, brisk, natural, or calm"
    );
}
