//! Tests for the screenshot a stopped run leaves: on the status that has room
//! for one, and in the report, taken before the task's surfaces are released.

use super::*;

use tinycomputer_bus::browser::{OutputId, OutputRef};

fn shot() -> OutputRef {
    OutputRef {
        id: OutputId::new("o-1"),
        total_bytes: 3,
        sha256: "abc".to_owned(),
        media_type: "image/png".to_owned(),
        width: 1,
        height: 1,
    }
}

fn with_shot(replies: Vec<DesktopResponse>) -> (Tasks, Arc<Script>) {
    let (tasks, script) = controller(replies);
    *script.shot.lock().unwrap() = Some(shot());
    (tasks, script)
}

fn paying() -> DesktopResponse {
    finished_run(
        FlowStopReason::StoppedBeforeDestructive,
        vec![step(
            "1",
            "stop_before",
            "paying for the booking",
            StepOutcome::Gated,
            "found it",
        )],
        &[],
        Some("Pay ₹6,840"),
    )
}

fn one_step() -> serde_json::Value {
    json!({"app": "browser", "steps": [{"stop_before": "paying for the booking"}]})
}

#[tokio::test]
async fn a_payment_checkpoint_shows_the_screen_it_stopped_on() {
    let (tasks, _) = with_shot(vec![paying()]);
    let view = start(&tasks, one_step(), &[]);
    let stopped = settle(&tasks, &view.id).await;
    let TaskStatus::Checkpoint { screenshot, .. } = &stopped.status else {
        panic!("{:?}", stopped.status);
    };
    assert_eq!(screenshot.as_ref(), Some(&shot()));
    assert_eq!(tasks.report(&view.id).data.unwrap().artifacts, [shot()]);
}

#[tokio::test]
async fn a_finished_task_keeps_its_last_screen_after_release() {
    let (tasks, script) = with_shot(vec![finished_run(
        FlowStopReason::Completed,
        vec![step("1", "do", "search for flights", StepOutcome::Done, "")],
        &[],
        None,
    )]);
    let view = start(
        &tasks,
        json!({"app": "browser", "steps": ["search for flights"]}),
        &[],
    );
    let done = settle(&tasks, &view.id).await;
    assert!(
        matches!(done.status, TaskStatus::Done { .. }),
        "{:?}",
        done.status
    );
    assert_eq!(*script.released.lock().unwrap(), [view.id.clone()]);
    assert_eq!(tasks.report(&view.id).data.unwrap().artifacts, [shot()]);
}

#[tokio::test]
async fn a_failed_task_keeps_its_last_screen() {
    let (tasks, _) = with_shot(vec![failed_at_step_two()]);
    let view = start(
        &tasks,
        json!({"app": "browser", "steps": [{"browse": "https://flights.test"}, "search for flights"]}),
        &[],
    );
    let failed = settle(&tasks, &view.id).await;
    assert!(
        matches!(failed.status, TaskStatus::Failed { .. }),
        "{:?}",
        failed.status
    );
    assert_eq!(tasks.report(&view.id).data.unwrap().artifacts, [shot()]);
}

#[tokio::test]
async fn a_surface_that_cannot_capture_leaves_no_screenshot() {
    let (tasks, _) = controller(vec![paying()]);
    let view = start(&tasks, one_step(), &[]);
    let stopped = settle(&tasks, &view.id).await;
    let TaskStatus::Checkpoint { screenshot, .. } = &stopped.status else {
        panic!("{:?}", stopped.status);
    };
    assert!(screenshot.is_none());
    assert!(tasks.report(&view.id).data.unwrap().artifacts.is_empty());
}

#[tokio::test]
async fn a_task_waiting_for_input_takes_no_screenshot() {
    let (tasks, _) = with_shot(Vec::new());
    let view = start(
        &tasks,
        json!({"app": "browser", "steps": [{"enter": {"phone": "${phone}"}}]}),
        &[],
    );
    assert!(
        matches!(view.status, TaskStatus::NeedsInput { .. }),
        "{:?}",
        view.status
    );
    assert!(tasks.report(&view.id).data.unwrap().artifacts.is_empty());
}
