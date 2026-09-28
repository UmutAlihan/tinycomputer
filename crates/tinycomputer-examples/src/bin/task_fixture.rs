//! Runs a whole booking task on the travel fixture through the task
//! controller, with live Jev: search, pick the cheapest flight, fill the
//! traveller form (asking for the phone number it was not given), skip the
//! upsell, and stop at payment.
//!
//! Needs `OPENROUTER_API_KEY` for Jev. Run it in the Docker lab, never on the
//! host: `scripts/docker-lab -- crates/tinycomputer-examples/fixtures/run task_fixture`.
//! Set `TINYCOMPUTER_FLOW_STRATEGY=wide` to run it with the wide strategy.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::json;
use tinycomputer::Desktop;
use tinycomputer_browser::{AgentBrowser, Browser, BrowserSurface, SessionOptions};
use tinycomputer_bus::agent::{
    AwaitTaskRequest, ContinueTaskRequest, StartTaskRequest, TaskConstraints, TaskId, TaskStatus,
    TaskView,
};
use tinycomputer_bus::{JevConfig, RunFlowRequest};
use tinycomputer_engine::{FlowFuture, FlowRunner, JevRuntime, Tasks, TextFuture, Workspace};

struct Fixture {
    workspace: Workspace<Desktop, BrowserSurface>,
    jev: JevRuntime,
}

impl FlowRunner for Fixture {
    fn run(
        &self,
        _task: &TaskId,
        _constraints: &TaskConstraints,
        request: RunFlowRequest,
    ) -> FlowFuture {
        Box::pin(tinycomputer_engine::run_flow(
            self.workspace.clone(),
            self.jev.clone(),
            request,
        ))
    }

    fn visible_text(&self, _task: &TaskId) -> TextFuture {
        let workspace = self.workspace.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || workspace.visible_text())
                .await
                .unwrap_or_default()
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = std::env::var("OPENROUTER_API_KEY")
        .map_err(|_| "OPENROUTER_API_KEY is not set; Jev cannot run")?;
    let base = std::env::var("TINYCOMPUTER_FIXTURE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8765".to_owned());
    let config: JevConfig =
        serde_json::from_value(json!({"api_key": key, "provider": "open_router"}))?;
    let jev = JevRuntime::configure(&config).map_err(|error| error.message)?;
    let browser = BrowserSurface::new(
        Arc::new(Browser::new(Arc::new(AgentBrowser))),
        SessionOptions {
            executable: std::env::var("TINYCOMPUTER_BROWSER_EXECUTABLE").ok(),
            ..SessionOptions::default()
        },
        tokio::runtime::Handle::current(),
    )
    .with_perception(tinycomputer_examples::perception_from_env());
    let tasks = Tasks::new(Arc::new(Fixture {
        workspace: Workspace::new(Some(Desktop::new()), Some(browser.clone())),
        jev,
    }));
    let started = tasks.start(&request(&base)?);
    let view = started
        .data
        .ok_or_else(|| format!("the task did not start: {:?}", started.error))?;
    let id = view.id.clone();
    let view = follow(&tasks, view).await?;
    if let Some(report) = tasks.report(&id).data {
        for step in &report.steps {
            println!(
                "  {} {} [{:?}] {}",
                step.path, step.kind, step.outcome, step.note
            );
        }
    }
    browser.close();
    if matches!(view.status, TaskStatus::Checkpoint { ref reason, .. } if reason.contains("payment"))
    {
        println!("PASS stopped at payment");
        Ok(())
    } else {
        Err("FAIL did not stop at payment".into())
    }
}

/// The booking task: the flow, and every fact but the phone number.
fn request(base: &str) -> Result<StartTaskRequest, serde_json::Error> {
    let flow = json!({"app": "browser", "steps": [
        {"browse": format!("{base}/index.html")},
        {"enter": {"from": "${from}", "to": "${to}", "departure date": "${date}"}},
        "search for flights",
        {"wait_for": "flight results are listed"},
        {"pick": {"from": "the flight results", "by": "lowest price", "into": "flight"}},
        {"enter": {"first name": "${first name}", "last name": "${last name}", "email": "${email}", "mobile number": "${phone}"}},
        "continue past the traveller details",
        "skip the seat selection",
        {"stop_before": "paying for the booking"}
    ]});
    let facts = [
        ("from", "Delhi"),
        ("to", "Srinagar"),
        ("date", "14 October"),
        ("first name", "Asha"),
        ("last name", "Raina"),
        ("email", "asha@example.com"),
    ];
    Ok(StartTaskRequest {
        flow: Some(serde_json::from_value(flow)?),
        facts: facts
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        trace: true,
        budget: tinycomputer_bus::agent::TaskBudget {
            strategy: tinycomputer_examples::flow_strategy_from_env(),
            ..tinycomputer_bus::agent::TaskBudget::default()
        },
        ..StartTaskRequest::default()
    })
}

/// Follows the task as a caller would, supplying the phone number when asked.
async fn follow(tasks: &Tasks, mut view: TaskView) -> Result<TaskView, String> {
    let id = view.id.clone();
    loop {
        println!("[{}] {}", state(&view.status), view.summary);
        let reply = match &view.status {
            TaskStatus::NeedsInput { fields } => tasks.continue_task(ContinueTaskRequest {
                id: id.clone(),
                inputs: fields
                    .iter()
                    .map(|field| (field.name.clone(), "+91 98765 43210".to_owned()))
                    .collect::<BTreeMap<_, _>>(),
                ..ContinueTaskRequest::default()
            }),
            TaskStatus::Running => {
                tasks
                    .await_task(AwaitTaskRequest {
                        id: id.clone(),
                        timeout_ms: 60_000,
                    })
                    .await
            }
            _ => return Ok(view),
        };
        view = reply
            .data
            .ok_or_else(|| format!("the task stopped answering: {:?}", reply.error))?;
    }
}

fn state(status: &TaskStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| {
            value
                .get("state")
                .and_then(|state| state.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_default()
}
