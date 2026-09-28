//! Tests for the configuration parser and the builders.

use std::path::Path;

use serde_json::json;

use crate::desktop::Desktop;

#[test]
fn a_default_desktop_is_sessionless_untraced_and_headless() {
    let desktop = Desktop::new();

    assert_eq!(desktop.session_id(), None);
    assert!(!desktop.is_tracing());
    assert!(!desktop.is_headed());
}

#[test]
fn a_null_configuration_is_the_default_configuration() {
    let desktop = Desktop::from_config(&serde_json::Value::Null).expect("null is accepted");

    assert_eq!(desktop.session_id(), None);
    assert!(!desktop.is_headed());
}

#[test]
fn an_empty_object_is_the_default_configuration() {
    // This is what the loader supplies when a host records no configuration at
    // all, so it must not be an error.
    let desktop = Desktop::from_config(&json!({})).expect("an empty object is accepted");

    assert_eq!(desktop.session_id(), None);
}

#[test]
fn every_recognized_configuration_field_is_read() {
    let desktop = Desktop::from_config(&json!({
        "session_id": "run-42",
        "trace_path": "/tmp/run.jsonl",
        "trace_strict": true,
        "headed": true,
    }))
    .expect("a full configuration is accepted");

    assert_eq!(desktop.session_id(), Some("run-42"));
    assert_eq!(desktop.trace_path(), Some(Path::new("/tmp/run.jsonl")));
    assert!(desktop.is_headed());
}

#[test]
fn an_unrecognized_configuration_field_is_ignored() {
    // A newer host configuring something this version does not know about must
    // still load, or a rollout has to be ordered.
    let desktop = Desktop::from_config(&json!({ "future_option": true }))
        .expect("an unknown field is ignored");

    assert_eq!(desktop.session_id(), None);
}

#[test]
fn a_non_object_configuration_is_rejected() {
    let error = Desktop::from_config(&json!([1, 2, 3])).expect_err("an array is not a config");

    assert!(matches!(error, crate::Error::ConfigNotAnObject));
}

#[test]
fn a_wrongly_typed_configuration_field_names_itself() {
    let error =
        Desktop::from_config(&json!({ "headed": "yes" })).expect_err("a string is not a boolean");

    assert!(matches!(
        error,
        crate::Error::ConfigFieldType {
            field: "headed",
            expected: "a boolean"
        }
    ));
}

#[test]
fn a_null_configuration_field_falls_back_to_its_default() {
    let desktop = Desktop::from_config(&json!({ "session_id": null, "headed": null }))
        .expect("explicit nulls are accepted");

    assert_eq!(desktop.session_id(), None);
    assert!(!desktop.is_headed());
}

#[test]
fn the_builders_set_what_they_name() {
    let desktop = Desktop::new()
        .with_session("run-1")
        .with_trace("/tmp/t.jsonl", true)
        .with_headed(true);

    assert_eq!(desktop.session_id(), Some("run-1"));
    assert!(desktop.is_tracing());
    assert!(desktop.is_headed());
}
