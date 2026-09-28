//! Tests for the module configuration: rejection, desktop availability, the
//! planner, the browser executable, and the cursor.

use crate::tinybus_module::DesktopService;
use serde_json::json;
use tinycomputer_bus::DesktopResponse;

#[test]
fn a_malformed_configuration_is_rejected_rather_than_defaulted() {
    let error = DesktopService::from_config(&json!({ "session_id": 7 }))
        .expect_err("a numeric session id is not a session id");

    assert!(error.to_string().contains("session_id"));
}

#[test]
fn the_desktop_is_available_only_with_accessibility() {
    use crate::tinybus_module::dispatch::desktop_availability;

    let reply = |state: serde_json::Value| {
        DesktopResponse::ok("permissions", json!({"accessibility": state}))
    };
    assert!(desktop_availability(&reply(json!({"state": "granted"}))).available);
    assert!(desktop_availability(&reply(json!({"state": "not_required"}))).available);
    let denied = desktop_availability(&reply(
        json!({"state": "denied", "suggestion": "open Settings"}),
    ));
    assert!(!denied.available);
    assert_eq!(denied.reason.as_deref(), Some("open Settings"));
    let bare = desktop_availability(&reply(json!({"state": "denied"})));
    assert_eq!(
        bare.reason.as_deref(),
        Some("grant the accessibility permission")
    );
    let unknown = desktop_availability(&reply(json!({"state": "unknown"})));
    assert!(unknown.reason.unwrap().contains("could not be read"));
    let failed = desktop_availability(&DesktopResponse::err(
        "permissions",
        tinycomputer_bus::DesktopError::new("PLATFORM_NOT_SUPPORTED", "no surfaces here"),
    ));
    assert_eq!(failed.reason.as_deref(), Some("no surfaces here"));
}

#[test]
fn a_planner_is_configured_from_private_configuration_only_with_a_key() {
    let service = DesktopService::from_config(&json!({"planner": {"api_key": "k"}})).unwrap();
    assert!(service.tasks.planner_configured());
    assert!(
        service.tasks.rescue_configured(),
        "the planner's key brings the rescuer"
    );
    assert!(service.tasks.output_configured(), "and the shaper");
    assert!(
        DesktopService::from_config(
            &json!({"planner": {"api_key": "k", "output_model": "openai/gpt-6-luna-pro"}})
        )
        .is_ok()
    );
    assert!(
        DesktopService::from_config(
            &json!({"planner": {"api_key": "k", "rescue_model": "openai/gpt-6-luna-pro"}})
        )
        .is_ok()
    );
    assert!(
        !DesktopService::from_config(&json!({}))
            .unwrap()
            .tasks
            .rescue_configured()
    );
    assert!(
        !DesktopService::from_config(&json!({}))
            .unwrap()
            .tasks
            .output_configured()
    );
    assert!(DesktopService::from_config(&json!({"planner": {"api_key": " "}})).is_err());
    assert!(DesktopService::from_config(&json!({"planner": {"model": "m"}})).is_err());
}

#[test]
fn the_browser_executable_is_configured_or_refused() {
    assert!(DesktopService::from_config(&json!({"browser": {}})).is_ok());
    assert!(
        DesktopService::from_config(&json!({"browser": {"executable": "/usr/bin/chromium"}}))
            .is_ok()
    );
    assert!(DesktopService::from_config(&json!({"browser": {"executable": 7}})).is_err());
    assert!(DesktopService::from_config(&json!({"browser": "chrome"})).is_err());
}

#[test]
fn the_cursor_is_configured_or_refused() {
    use crate::tinybus_module::config::cursor_config;
    use tinycomputer_browser::CursorPace;
    assert_eq!(
        cursor_config(&json!({})).unwrap().pace(),
        CursorPace::Natural
    );
    assert_eq!(
        cursor_config(&json!({"cursor": "calm"})).unwrap().pace(),
        CursorPace::Calm
    );
    assert!(
        cursor_config(&json!({"cursor": "off"}))
            .unwrap()
            .pace()
            .is_off()
    );
    let configured =
        cursor_config(&json!({"cursor": {"pace": "brisk", "overlay": "/opt/overlay"}}));
    assert_eq!(configured.unwrap().pace(), CursorPace::Brisk);
    assert_eq!(
        cursor_config(&json!({"cursor": {}})).unwrap().pace(),
        CursorPace::Natural
    );
    for wrong in [
        json!({"cursor": "frantic"}),
        json!({"cursor": true}),
        json!({"cursor": {"pace": 3}}),
        json!({"cursor": {"overlay": 3}}),
    ] {
        assert!(DesktopService::from_config(&wrong).is_err(), "{wrong}");
    }
    assert!(DesktopService::from_config(&json!({"cursor": "off"})).is_ok());
}
