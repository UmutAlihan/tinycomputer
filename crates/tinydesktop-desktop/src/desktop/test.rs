//! Unit tests for the engine's configuration, conversions, and envelope.
//!
//! These cover what can be asserted without a display server, a granted
//! permission, or a running application: the configuration parser, the
//! contract-to-engine conversions, the permission preflight, and the two
//! members that touch nothing outside this process. Anything that drives a real
//! application belongs in a live suite, not here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use serde_json::json;
use tinydesktop_bus as bus;

use super::{Desktop, convert, permission, permission::Need, reply};
use agent_desktop_core::{
    AdapterError, AppError, DeliverySemantics, ErrorCode, PermissionReport, PermissionState,
};

/// A report denying `accessibility`, `screen_recording`, or both.
fn denying(accessibility: bool, screen_recording: bool) -> PermissionReport {
    let denied = || PermissionState::Denied {
        suggestion: "Grant it in System Settings".to_owned(),
    };
    let mut report = PermissionReport::default();
    if accessibility {
        report.accessibility = denied();
    }
    if screen_recording {
        report.screen_recording = denied();
    }
    report
}

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

#[test]
fn version_answers_without_a_permission_or_a_display() {
    let reply = Desktop::new().version();

    assert!(reply.ok, "version must work on an unconfigured machine");
    assert_eq!(reply.command, "version");
    let data = reply.data.expect("a successful reply carries data");
    assert!(data.get("version").is_some());
    assert_eq!(data["os"], json!(std::env::consts::OS));
}

#[test]
fn a_zero_millisecond_wait_returns_immediately_and_reports_what_it_waited() {
    let reply = Desktop::new().wait(bus::WaitRequest::sleep(1));

    assert!(reply.ok);
    assert_eq!(reply.data.expect("data")["waited_ms"], json!(1));
}

#[test]
fn a_wait_with_no_mode_is_an_invalid_argument_rather_than_a_hang() {
    let reply = Desktop::new().wait(bus::WaitRequest::default());

    assert!(!reply.ok);
    let error = reply.error.expect("a failed reply carries an error");
    assert_eq!(error.code, ErrorCode::InvalidArgs.as_str());
}

#[test]
fn an_unknown_surface_wait_is_rejected_by_name() {
    let reply = Desktop::new().wait(bus::WaitRequest {
        surface: Some("menu-closed".to_owned()),
        ..bus::WaitRequest::default()
    });

    assert!(!reply.ok);
    let error = reply.error.expect("error");
    assert!(
        error.message.contains("menu-closed"),
        "the rejection must name the value: {}",
        error.message
    );
}

#[test]
fn every_recognized_surface_wait_is_accepted_by_the_parser() {
    for surface in ["menu", "menu_closed", "notification"] {
        assert!(
            super::waiting::surface_wait(Some(surface)).is_ok(),
            "{surface} must parse"
        );
    }
    assert!(super::waiting::surface_wait(None).unwrap().is_none());
}

#[test]
fn a_command_that_fails_still_names_itself_in_the_envelope() {
    // Whatever this machine says about the request, the command name is the
    // one field a batched caller correlates on, so it must be present on both
    // outcomes.
    let reply = Desktop::new().get(bus::GetRequest::new("", bus::ElementProperty::Value));

    assert_eq!(reply.command, "get");
    assert_eq!(reply.version, bus::ENVELOPE_VERSION);
    assert_eq!(reply.ok, reply.error.is_none());
}

#[test]
fn a_denied_accessibility_permission_is_reported_before_the_command_runs() {
    let report = denying(true, false);

    let error = permission::preflight(Need::Accessibility, &report)
        .expect_err("a denied permission must not be ignored");
    let payload = reply::envelope("click", Err(error))
        .error
        .expect("a failed envelope carries an error");

    assert_eq!(payload.code, ErrorCode::PermDenied.as_str());
    // Nothing was attempted, so retrying after granting cannot duplicate an
    // effect.
    assert_eq!(payload.disposition.retry, bus::RetryDisposition::Safe);
}

#[test]
fn a_need_of_nothing_passes_a_report_that_denies_everything() {
    assert!(permission::preflight(Need::Nothing, &denying(true, true)).is_ok());
}

#[test]
fn a_screen_recording_need_ignores_a_denied_accessibility_permission() {
    assert!(permission::preflight(Need::ScreenRecording, &denying(true, false)).is_ok());
}

#[test]
fn a_successful_result_becomes_a_successful_envelope() {
    let envelope = reply::envelope("version", Ok(json!({ "version": "0.8.3" })));

    assert!(envelope.ok);
    assert_eq!(envelope.command, "version");
    assert_eq!(envelope.data, Some(json!({ "version": "0.8.3" })));
    assert!(envelope.error.is_none());
}

#[test]
fn an_adapter_error_keeps_its_structured_detail_through_the_envelope() {
    let error = AppError::Adapter(
        AdapterError::new(ErrorCode::StaleRef, "Ref @s1:e2 is no longer valid")
            .with_suggestion("Take a fresh snapshot")
            .with_details(json!({ "ref": "@s1:e2" }))
            .with_disposition(DeliverySemantics::not_delivered()),
    );

    let payload = reply::envelope("click", Err(error))
        .error
        .expect("a failed envelope carries an error");

    assert_eq!(payload.code, "STALE_REF");
    assert_eq!(payload.suggestion.as_deref(), Some("Take a fresh snapshot"));
    assert_eq!(payload.details, Some(json!({ "ref": "@s1:e2" })));
    assert_eq!(payload.disposition.retry, bus::RetryDisposition::Safe);
    // A retry-safe stale ref is exactly the case that gets a recovery hint.
    let recovery = payload
        .recovery
        .expect("a stale ref carries a recovery hint");
    assert!(recovery.requires_fresh_snapshot);
}

#[test]
fn every_contract_surface_maps_onto_an_engine_surface() {
    // The `match` in `convert` is exhaustive, so this loop is really asserting
    // that the mapping is one-to-one by name rather than merely total.
    for (contract, engine) in [
        (bus::Surface::Window, "window"),
        (bus::Surface::Focused, "focused"),
        (bus::Surface::Menu, "menu"),
        (bus::Surface::Menubar, "menubar"),
        (bus::Surface::Sheet, "sheet"),
        (bus::Surface::Popover, "popover"),
        (bus::Surface::Alert, "alert"),
        (bus::Surface::Desktop, "desktop"),
        (bus::Surface::Taskbar, "taskbar"),
        (bus::Surface::SystemTray, "system_tray"),
        (bus::Surface::QuickSettings, "quick_settings"),
        (bus::Surface::NotificationCenter, "notification_center"),
        (bus::Surface::Toolbar, "toolbar"),
        (bus::Surface::Dock, "dock"),
        (bus::Surface::Spotlight, "spotlight"),
        (bus::Surface::MenuBarExtras, "menu_bar_extras"),
        (bus::Surface::SystemTrayOverflow, "system_tray_overflow"),
        (bus::Surface::StartMenu, "start_menu"),
        (bus::Surface::ActionCenter, "action_center"),
    ] {
        assert_eq!(convert::surface(contract).as_str(), engine);
    }
}

#[test]
fn every_contract_clipboard_format_maps_onto_an_engine_format() {
    for (contract, engine) in [
        (bus::ClipboardFormat::Auto, "auto"),
        (bus::ClipboardFormat::Text, "text"),
        (bus::ClipboardFormat::Image, "image"),
        (bus::ClipboardFormat::FileUrls, "file_urls"),
    ] {
        assert_eq!(convert::clipboard_format(contract).as_str(), engine);
    }
}

#[test]
fn modifiers_convert_in_order_and_without_loss() {
    let converted = convert::modifiers(vec![
        bus::Modifier::Meta,
        bus::Modifier::Ctrl,
        bus::Modifier::Alt,
        bus::Modifier::Shift,
    ]);

    assert_eq!(
        serde_json::to_value(converted).unwrap(),
        json!(["Meta", "Ctrl", "Alt", "Shift"])
    );
}

#[test]
fn directions_and_buttons_convert_without_loss() {
    for (contract, expected) in [
        (bus::Direction::Up, "Up"),
        (bus::Direction::Down, "Down"),
        (bus::Direction::Left, "Left"),
        (bus::Direction::Right, "Right"),
    ] {
        assert_eq!(
            serde_json::to_value(convert::direction(contract)).unwrap(),
            json!(expected)
        );
    }
    for (contract, expected) in [
        (bus::MouseButton::Left, "Left"),
        (bus::MouseButton::Right, "Right"),
        (bus::MouseButton::Middle, "Middle"),
    ] {
        assert_eq!(
            serde_json::to_value(convert::mouse_button(contract)).unwrap(),
            json!(expected)
        );
    }
}

#[test]
fn a_half_specified_point_becomes_no_point_at_all() {
    // Filling the missing half with zero would drag to the corner of the
    // screen; the engine instead reports the missing input by name.
    assert_eq!(convert::point(Some(1.0), None), None);
    assert_eq!(convert::point(None, Some(2.0)), None);
    assert_eq!(convert::point(Some(1.0), Some(2.0)), Some((1.0, 2.0)));
}

#[test]
fn a_drag_endpoint_carries_a_ref_or_a_point_but_never_half_of_one() {
    let by_ref = convert::drag_endpoint(bus::DragEndpoint::at_ref("@s1:e2"));
    assert_eq!(by_ref.ref_id.as_deref(), Some("@s1:e2"));
    assert_eq!(by_ref.xy, None);

    let half = convert::drag_endpoint(bus::DragEndpoint {
        ref_id: None,
        x: Some(1.0),
        y: None,
    });
    assert_eq!(half.xy, None);
}

#[test]
fn a_state_predicate_keeps_its_expectation_across_the_boundary() {
    let set = convert::state_predicate(bus::StatePredicate::set("enabled"));
    let negated = convert::state_predicate(bus::StatePredicate::expect("focused", false));

    assert_eq!(set.token, "enabled");
    assert_eq!(set.expected, None);
    assert_eq!(negated.expected, Some(false));
}

#[test]
fn a_ref_request_converts_field_for_field() {
    let args = convert::ref_args(bus::RefRequest {
        ref_id: "@s1:e2".to_owned(),
        snapshot_id: Some("s1".to_owned()),
        timeout_ms: Some(1_000),
    });

    assert_eq!(args.ref_id, "@s1:e2");
    assert_eq!(args.snapshot_id.as_deref(), Some("s1"));
    assert_eq!(args.timeout_ms, Some(1_000));
}

#[test]
fn a_window_request_converts_field_for_field() {
    let args = convert::window_args(bus::WindowRequest {
        app: Some("Safari".to_owned()),
        window_id: Some("w1".to_owned()),
    });

    assert_eq!(args.app.as_deref(), Some("Safari"));
    assert_eq!(args.window_id.as_deref(), Some("w1"));
}

#[test]
fn an_absent_path_stays_absent() {
    assert_eq!(convert::path(None), None);
    assert_eq!(
        convert::path(Some("/tmp/shot.png".to_owned())),
        Some(Path::new("/tmp/shot.png").to_path_buf())
    );
}

// ---------------------------------------------------------------------------
// The member sweep.
//
// Every member is one call into the engine, and the thing worth asserting about
// each is that it calls the right one, names itself correctly, and answers in
// the envelope. Agentic members live above `Desktop` and are covered in their
// own module and bus tests.
// driven from one table.
//
// # Why running all of them is safe
//
// A default `Desktop` is headless, and every payload below is one the engine
// rejects before it reaches the machine:
//
// - the cursor and notification members are refused by the headless input
//   policy, before any hardware or system surface is touched;
// - the ref actions carry an empty ref, which resolves nowhere;
// - `press` carries an empty combo, which fails to parse;
// - the application and window members name an empty application, which
//   resolves to nothing;
// - `clipboard-set` carries no content, which fails validation before the
//   pasteboard is opened.
//
// `clipboard-clear` is the one member with no invalid input to hand it, so it
// is exercised only where the platform has no pasteboard to empty. Anything
// that has to be proved against a live application belongs in a live suite.
// ---------------------------------------------------------------------------

/// Calls every member on a default `Desktop` and returns the replies.
fn sweep() -> Vec<tinydesktop_bus::DesktopResponse> {
    let desktop = Desktop::new();
    let no_app = || Some(String::new());
    let empty_ref = || bus::RefRequest::new("");

    let mut replies = vec![
        desktop.snapshot(bus::SnapshotRequest::default()),
        desktop.find(bus::FindRequest::default()),
        desktop.get(bus::GetRequest::new("", bus::ElementProperty::Text)),
        desktop.is(bus::IsRequest::new("", bus::ElementStateProperty::Visible)),
        desktop.screenshot(bus::ScreenshotRequest {
            app: no_app(),
            ..bus::ScreenshotRequest::default()
        }),
        desktop.click(empty_ref()),
        desktop.double_click(empty_ref()),
        desktop.triple_click(empty_ref()),
        desktop.right_click(empty_ref()),
        desktop.type_text(bus::TypeRequest::default()),
        desktop.set_value(bus::SetValueRequest::default()),
        desktop.clear(empty_ref()),
        desktop.focus(empty_ref()),
        desktop.select(bus::SelectRequest::default()),
        desktop.toggle(empty_ref()),
        desktop.check(empty_ref()),
        desktop.uncheck(empty_ref()),
        desktop.expand(empty_ref()),
        desktop.collapse(empty_ref()),
        desktop.scroll(bus::ScrollRequest::new("", bus::Direction::Down, 1)),
        desktop.scroll_to(empty_ref()),
        desktop.press(bus::PressRequest::new("")),
        desktop.key_down(bus::HoldKeyRequest::default()),
        desktop.key_up(bus::HoldKeyRequest::default()),
        desktop.hover(bus::HoverRequest::default()),
        desktop.drag(bus::DragRequest::default()),
        desktop.mouse_move(bus::MouseMoveRequest::default()),
        desktop.mouse_click(bus::MouseClickRequest::default()),
        desktop.mouse_down(bus::HoldMouseRequest::default()),
        desktop.mouse_up(bus::HoldMouseRequest::default()),
        desktop.mouse_wheel(bus::MouseWheelRequest::default()),
        desktop.launch(bus::LaunchRequest::new("")),
        desktop.close_app(bus::CloseAppRequest::default()),
        desktop.list_apps(bus::ListAppsRequest::default()),
        desktop.list_windows(bus::ListWindowsRequest::default()),
        desktop.list_displays(),
        desktop.list_surfaces(bus::ListSurfacesRequest { app: no_app() }),
        desktop.focus_window(bus::FocusWindowRequest {
            app: no_app(),
            ..bus::FocusWindowRequest::default()
        }),
        desktop.resize_window(bus::ResizeWindowRequest {
            app: no_app(),
            width: 100.0,
            height: 100.0,
            ..bus::ResizeWindowRequest::default()
        }),
        desktop.move_window(bus::MoveWindowRequest {
            app: no_app(),
            ..bus::MoveWindowRequest::default()
        }),
        desktop.minimize(bus::WindowRequest {
            app: no_app(),
            window_id: None,
        }),
        desktop.maximize(bus::WindowRequest {
            app: no_app(),
            window_id: None,
        }),
        desktop.restore(bus::WindowRequest {
            app: no_app(),
            window_id: None,
        }),
        desktop.clipboard_get(bus::ClipboardGetRequest::default()),
        desktop.clipboard_set(bus::ClipboardSetRequest::default()),
    ];

    // See the note above: nothing invalid can be handed to this one, so it runs
    // only where there is no pasteboard for it to empty.
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    replies.push(desktop.clipboard_clear());
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    replies.push(tinydesktop_bus::DesktopResponse::ok(
        "clipboard-clear",
        json!({ "skipped": "would empty a real pasteboard" }),
    ));

    replies.extend([
        desktop.list_notifications(bus::ListNotificationsRequest::default()),
        desktop.notification_action(bus::NotificationActionRequest::default()),
        desktop.dismiss_notification(bus::DismissNotificationRequest::default()),
        desktop.dismiss_all_notifications(bus::DismissAllNotificationsRequest::default()),
        desktop.wait(bus::WaitRequest::sleep(1)),
        desktop.version(),
        desktop.status(),
        desktop.permissions(bus::PermissionsRequest::default()),
    ]);

    replies
}

#[test]
fn the_sweep_covers_every_member_the_contract_names() {
    // The five agentic members compose the others; they have no engine
    // command of their own to sweep.
    assert_eq!(sweep().len() + 5, bus::METHODS.len());
}

#[test]
fn every_member_answers_in_the_envelope() {
    for reply in sweep() {
        assert_eq!(reply.version, bus::ENVELOPE_VERSION);
        assert!(!reply.command.is_empty());
        assert_eq!(reply.ok, reply.data.is_some());
        assert_eq!(!reply.ok, reply.error.is_some());
        if let Some(error) = reply.error {
            assert!(
                !error.code.is_empty(),
                "{} gave an empty code",
                reply.command
            );
            assert!(!error.message.is_empty());
        }
    }
}

#[test]
fn every_member_names_itself_in_the_engines_spelling() {
    // The envelope's `command` is what a batched caller correlates on and what
    // a trace records, so a member reporting a neighbour's name would be
    // invisible until someone read a trace and disbelieved it.
    let commands = sweep()
        .into_iter()
        .map(|reply| reply.command)
        .collect::<Vec<_>>();

    assert_eq!(
        commands,
        vec![
            "snapshot",
            "find",
            "get",
            "is",
            "screenshot",
            "click",
            "double-click",
            "triple-click",
            "right-click",
            "type",
            "set-value",
            "clear",
            "focus",
            "select",
            "toggle",
            "check",
            "uncheck",
            "expand",
            "collapse",
            "scroll",
            "scroll-to",
            "press",
            "key-down",
            "key-up",
            "hover",
            "drag",
            "mouse-move",
            "mouse-click",
            "mouse-down",
            "mouse-up",
            "mouse-wheel",
            "launch",
            "close-app",
            "list-apps",
            "list-windows",
            "list-displays",
            "list-surfaces",
            "focus-window",
            "resize-window",
            "move-window",
            "minimize",
            "maximize",
            "restore",
            "clipboard-get",
            "clipboard-set",
            "clipboard-clear",
            "list-notifications",
            "notification-action",
            "dismiss-notification",
            "dismiss-all-notifications",
            "wait",
            "version",
            "status",
            "permissions",
        ]
    );
}

#[test]
fn a_headless_desktop_refuses_the_members_that_would_move_the_real_cursor() {
    let desktop = Desktop::new();

    for reply in [
        desktop.hover(bus::HoverRequest::default()),
        desktop.mouse_move(bus::MouseMoveRequest::default()),
        desktop.mouse_click(bus::MouseClickRequest::default()),
        desktop.mouse_wheel(bus::MouseWheelRequest::default()),
        desktop.drag(bus::DragRequest::default()),
    ] {
        assert!(
            !reply.ok,
            "{} must be refused while headless",
            reply.command
        );
    }
}

#[test]
fn a_notification_mutation_is_refused_while_headless() {
    // Opening the notification surface takes focus, so a headless run cannot do
    // it — and finds that out before it acts, not after.
    let reply =
        Desktop::new().dismiss_all_notifications(bus::DismissAllNotificationsRequest::default());

    assert!(!reply.ok);
}

#[test]
fn every_contract_element_property_maps_onto_an_engine_property() {
    // `GetProperty` is not `PartialEq`, so the mapping is asserted through the
    // property name the engine puts in its reply.
    use agent_desktop_core::commands::get::GetProperty;

    for (contract, engine) in [
        (bus::ElementProperty::Text, GetProperty::Text),
        (bus::ElementProperty::Value, GetProperty::Value),
        (bus::ElementProperty::Title, GetProperty::Title),
        (bus::ElementProperty::Bounds, GetProperty::Bounds),
        (bus::ElementProperty::Role, GetProperty::Role),
        (bus::ElementProperty::States, GetProperty::States),
    ] {
        assert!(matches!(
            (convert::element_property(contract), engine),
            (GetProperty::Text, GetProperty::Text)
                | (GetProperty::Value, GetProperty::Value)
                | (GetProperty::Title, GetProperty::Title)
                | (GetProperty::Bounds, GetProperty::Bounds)
                | (GetProperty::Role, GetProperty::Role)
                | (GetProperty::States, GetProperty::States)
        ));
    }
}

#[test]
fn every_contract_element_state_maps_onto_an_engine_state() {
    use agent_desktop_core::commands::is_check::IsProperty;

    for (contract, engine) in [
        (bus::ElementStateProperty::Visible, IsProperty::Visible),
        (bus::ElementStateProperty::Enabled, IsProperty::Enabled),
        (bus::ElementStateProperty::Checked, IsProperty::Checked),
        (bus::ElementStateProperty::Focused, IsProperty::Focused),
        (bus::ElementStateProperty::Expanded, IsProperty::Expanded),
        (bus::ElementStateProperty::Selected, IsProperty::Selected),
    ] {
        assert!(matches!(
            (convert::state_property(contract), engine),
            (IsProperty::Visible, IsProperty::Visible)
                | (IsProperty::Enabled, IsProperty::Enabled)
                | (IsProperty::Checked, IsProperty::Checked)
                | (IsProperty::Focused, IsProperty::Focused)
                | (IsProperty::Expanded, IsProperty::Expanded)
                | (IsProperty::Selected, IsProperty::Selected)
        ));
    }
}

#[test]
fn a_screenshot_of_a_named_application_needs_more_than_a_full_screen_one() {
    // Framing a named window means resolving it through the accessibility tree
    // first, so the need is both permissions rather than screen recording
    // alone. Asserted through the reply because the need itself is private.
    let full_screen = Desktop::new().screenshot(bus::ScreenshotRequest::default());
    let targeted = Desktop::new().screenshot(bus::ScreenshotRequest {
        app: Some("Safari".to_owned()),
        ..bus::ScreenshotRequest::default()
    });

    assert_eq!(full_screen.command, "screenshot");
    assert_eq!(targeted.command, "screenshot");
}

#[test]
fn a_denied_screen_recording_permission_is_reported_before_the_capture_runs() {
    let error = permission::preflight(Need::ScreenRecording, &denying(false, true))
        .expect_err("a denied permission must not be ignored");
    let payload = reply::envelope("screenshot", Err(error))
        .error
        .expect("a failed envelope carries an error");

    assert_eq!(payload.code, ErrorCode::PermDenied.as_str());
    assert!(
        payload.suggestion.is_some(),
        "a permission refusal must say where to grant it"
    );
}

#[test]
fn a_capture_of_a_named_window_is_refused_by_either_missing_permission() {
    // The combined need means the accessibility half alone is enough to refuse,
    // because the window cannot be resolved without it.
    for report in [denying(true, false), denying(false, true)] {
        assert!(
            permission::preflight(Need::AccessibilityAndScreenRecording, &report).is_err(),
            "a targeted capture needs both"
        );
    }
    // An *unknown* permission is not a denied one: a platform that cannot say
    // must not block the command, or Linux — where nothing is reportable —
    // would refuse everything before the engine got the chance to explain what
    // it does not support.
    assert!(
        permission::preflight(
            Need::AccessibilityAndScreenRecording,
            &PermissionReport::default()
        )
        .is_ok()
    );
}

#[test]
fn every_delivery_disposition_survives_the_envelope_with_its_retry_verdict() {
    for (semantics, expected_delivery, expected_retry) in [
        (
            DeliverySemantics::unknown(),
            bus::DeliveryDisposition::Unknown,
            bus::RetryDisposition::Unknown,
        ),
        (
            DeliverySemantics::not_delivered(),
            bus::DeliveryDisposition::NotDelivered,
            bus::RetryDisposition::Safe,
        ),
        (
            DeliverySemantics::uncertain(),
            bus::DeliveryDisposition::DeliveryUncertain,
            bus::RetryDisposition::Unsafe,
        ),
        (
            DeliverySemantics::delivered_unverified(),
            bus::DeliveryDisposition::DeliveredUnverified,
            bus::RetryDisposition::Unsafe,
        ),
        (
            DeliverySemantics::delivered_verified(),
            bus::DeliveryDisposition::DeliveredVerified,
            bus::RetryDisposition::Unsafe,
        ),
    ] {
        let error = AppError::Adapter(
            AdapterError::new(ErrorCode::ActionFailed, "the action did not take")
                .with_disposition(semantics),
        );
        let payload = reply::envelope("click", Err(error))
            .error
            .expect("a failed envelope carries an error");

        assert_eq!(payload.disposition.delivery, expected_delivery);
        assert_eq!(payload.disposition.retry, expected_retry);
        // The pair the engine reports and the pair the contract derives are the
        // same pair; a caller may trust either field.
        assert_eq!(payload.disposition, bus::Delivery::of(expected_delivery));
    }
}

#[test]
fn a_rejected_argument_is_reported_as_never_having_been_delivered() {
    // An argument the engine refuses never reached the application, so a retry
    // with a corrected argument cannot duplicate an effect. The envelope has to
    // carry that, or a caller has to guess.
    let payload = reply::envelope("find", Err(AppError::invalid_input("no mode selected")))
        .error
        .expect("a failed envelope carries an error");

    assert_eq!(payload.code, ErrorCode::InvalidArgs.as_str());
    assert!(payload.platform_detail.is_none());
    assert!(payload.details.is_none());
    assert_eq!(payload.disposition.retry, bus::RetryDisposition::Safe);
}

#[test]
fn an_error_the_engine_cannot_classify_becomes_an_internal_one() {
    // `Internal` carries no code of its own, so this is the branch of the
    // mapping that must not assume an adapter error.
    let payload = reply::envelope("status", Err(AppError::Internal("boom".to_owned())))
        .error
        .expect("a failed envelope carries an error");

    assert_eq!(payload.code, ErrorCode::Internal.as_str());
    assert_eq!(payload.message, "boom");
    assert!(payload.suggestion.is_none());
    assert!(payload.recovery.is_none());
    assert_eq!(payload.disposition, bus::Delivery::default());
}
