//! Tests for the crate error's message.

use super::Error;

#[test]
fn an_unknown_pace_names_itself_and_the_choices() {
    let error = Error::UnknownPace {
        name: "frantic".to_owned(),
    };
    assert_eq!(
        error.to_string(),
        "unknown cursor pace `frantic`, expected off, brisk, natural, or calm"
    );
}
