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
async fn a_payment_checkpoint_keeps_its_screen_in_the_report_only() {
    let (tasks, _) = with_shot(vec![paying()]);
    let view = start(&tasks, one_step(), &[]);
    let stopped = settle(&tasks, &view.id).await;
    let TaskStatus::Checkpoint { screenshot, .. } = &stopped.status else {
        panic!("{:?}", stopped.status);
    };
    // A view also travels through the non-confidential AwaitTask and
    // ListTasks, so the handle stays out of it.
    assert!(screenshot.is_none());
    let listed = tasks.list().data.unwrap();
    assert!(
        serde_json::to_string(&listed)
            .unwrap()
            .find("o-1")
            .is_none()
    );
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
    assert_eq!(
        *script.released.lock().unwrap(),
        std::slice::from_ref(&view.id)
    );
    assert_eq!(tasks.report(&view.id).data.unwrap().artifacts, [shot()]);
    // The screenshot is taken while the surface is still held.
    assert_eq!(*script.events.lock().unwrap(), ["capture", "release"]);
}

#[tokio::test(start_paused = true)]
async fn a_capture_that_never_answers_does_not_hold_the_task_back() {
    let (tasks, script) = with_shot(vec![finished_run(
        FlowStopReason::Completed,
        vec![step("1", "do", "search for flights", StepOutcome::Done, "")],
        &[],
        None,
    )]);
    script
        .stuck
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let view = start(
        &tasks,
        json!({"app": "browser", "steps": ["search for flights"]}),
        &[],
    );
    // Paused time jumps each wait, so the capture's timeout passes at once.
    let mut current = view.clone();
    for _ in 0..10 {
        if current.status.is_final() {
            break;
        }
        current = settle(&tasks, &view.id).await;
    }
    assert!(
        matches!(current.status, TaskStatus::Done { .. }),
        "{:?}",
        current.status
    );
    assert_eq!(
        *script.released.lock().unwrap(),
        std::slice::from_ref(&view.id)
    );
    assert_eq!(
        tasks.report(&view.id).data.unwrap().artifacts,
        [] as [tinycomputer_bus::browser::OutputRef; 0]
    );
}

#[tokio::test(start_paused = true)]
async fn a_task_cut_off_by_its_time_budget_keeps_its_last_screen() {
    let (tasks, script) = with_shot(Vec::new());
    let reply = tasks.start(&StartTaskRequest {
        flow: Some(flow(
            json!({"app": "browser", "steps": ["search for flights"]}),
        )),
        budget: tinycomputer_bus::agent::TaskBudget {
            max_elapsed_ms: Some(1),
            ..tinycomputer_bus::agent::TaskBudget::default()
        },
        ..StartTaskRequest::default()
    });
    let view = reply.data.unwrap();
    let mut current = view.clone();
    for _ in 0..10 {
        if current.status.is_final() {
            break;
        }
        current = settle(&tasks, &view.id).await;
    }
    assert!(
        matches!(current.status, TaskStatus::Failed { .. }),
        "{:?}",
        current.status
    );
    assert_eq!(tasks.report(&view.id).data.unwrap().artifacts, [shot()]);
    assert_eq!(*script.events.lock().unwrap(), ["capture", "release"]);
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
    assert_eq!(
        tasks.report(&view.id).data.unwrap().artifacts,
        [] as [tinycomputer_bus::browser::OutputRef; 0]
    );
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
    assert_eq!(
        tasks.report(&view.id).data.unwrap().artifacts,
        [] as [tinycomputer_bus::browser::OutputRef; 0]
    );
}
