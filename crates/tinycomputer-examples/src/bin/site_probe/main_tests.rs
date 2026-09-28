//! Tests for the `site_probe` binary: control lines, editable focus, and point\nparsing.

use serde_json::json;
use tinycomputer_core::surface::Candidate;

use super::{control_line, editable_focus, parse_point};

#[test]
fn parses_a_point_of_two_numbers() {
    assert_eq!(parse_point("120,48.5"), Ok((120.0, 48.5)));
    assert_eq!(parse_point(" 120 , 48 "), Ok((120.0, 48.0)));
}

#[test]
fn rejects_a_point_that_is_not_two_numbers() {
    for point in ["120", "x,48", "120,y", ",48", "120,", "NaN,1", "1,inf"] {
        assert!(parse_point(point).is_err(), "{point:?} parsed");
    }
}

#[test]
fn control_line_leaves_the_value_out() {
    let node = Candidate {
        ref_id: "e7".to_owned(),
        role: "textbox".to_owned(),
        name: Some("Card number".to_owned()),
        value: Some(json!("4111111111111111")),
        ..Candidate::default()
    };
    let line = control_line(&node);
    assert_eq!(line, r#"  e7 textbox "Card number""#);
    assert!(!line.contains("4111"));
}

#[test]
fn names_the_focused_field_only_when_it_is_editable() {
    assert_eq!(
        editable_focus(&json!({"result": "input[type=password] \"Password\""})),
        Some("input[type=password] \"Password\"".to_owned())
    );
    assert_eq!(editable_focus(&json!({"result": null})), None);
    assert_eq!(editable_focus(&json!({"result": false})), None);
    assert_eq!(editable_focus(&json!({})), None);
}
