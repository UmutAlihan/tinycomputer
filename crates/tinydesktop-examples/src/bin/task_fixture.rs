//! Runs a whole booking task on the travel fixture through the task
//! controller, with live Jev: search, pick the cheapest flight, fill the
//! traveller form (asking for the phone number it was not given), skip the
//! upsell, and stop at payment.
//!
//! Needs `OPENROUTER_API_KEY` for Jev. Run it in the Docker lab, never on the
//! host: `scripts/docker-lab -- crates/tinydesktop-examples/fixtures/run task_fixture`.

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::sync::Arc;

use serde_json::json;
use tinydesktop::Desktop;
use tinydesktop_browser::{AgentBrowser, Browser, BrowserSurface, SessionOptions};
use tinydesktop_bus::agent::{
    AwaitTaskRequest, ContinueTaskRequest, StartTaskRequest, TaskConstraints, TaskId, TaskStatus,
};
use tinydesktop_bus::{JevConfig, RunFlowRequest};
use tinydesktop_engine::{FlowFuture, FlowRunner, JevRuntime, Tasks, TextFuture, Workspace};

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
        Box::pin(tinydesktop_engine::run_flow(
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
async fn main() -> ExitCode {
    let Ok(key) = std::env::var("OPENROUTER_API_KEY") else {
        eprintln!("OPENROUTER_API_KEY is not set; Jev cannot run");
        return ExitCode::FAILURE;
    };
    let base = std::env::var("TINYDESKTOP_FIXTURE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8765".to_owned());
    let config: JevConfig =
        serde_json::from_value(json!({"api_key": key, "provider": "open_router"}))
            .expect("a Jev configuration from a key");
    let jev = JevRuntime::configure(&config).expect("Jev configures");
    let browser = BrowserSurface::new(
        Arc::new(Browser::new(Arc::new(AgentBrowser))),
        SessionOptions {
            executable: std::env::var("TINYDESKTOP_BROWSER_EXECUTABLE").ok(),
            ..SessionOptions::default()
        },
        tokio::runtime::Handle::current(),
    );
    let tasks = Tasks::new(Arc::new(Fixture {
        workspace: Workspace::new(Desktop::new(), Some(browser.clone())),
        jev,
    }));

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
    let started = tasks.start(&StartTaskRequest {
        flow: Some(serde_json::from_value(flow).expect("the fixture flow parses")),
        facts: facts
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        trace: true,
        ..StartTaskRequest::default()
    });
    let Some(mut view) = started.data else {
        eprintln!("the task did not start: {:?}", started.error);
        return ExitCode::FAILURE;
    };
    let id = view.id.clone();
    loop {
        println!("[{}] {}", state(&view.status), view.summary);
        match &view.status {
            TaskStatus::NeedsInput { fields } => {
                let inputs = fields
                    .iter()
                    .map(|field| (field.name.clone(), "+91 98765 43210".to_owned()))
                    .collect::<BTreeMap<_, _>>();
                view = tasks
                    .continue_task(ContinueTaskRequest {
                        id: id.clone(),
                        inputs,
                        ..ContinueTaskRequest::default()
                    })
                    .data
                    .expect("inputs are accepted");
            }
            TaskStatus::Running => {
                view = tasks
                    .await_task(AwaitTaskRequest {
                        id: id.clone(),
                        timeout_ms: 60_000,
                    })
                    .await
                    .data
                    .expect("the task exists");
            }
            _ => break,
        }
    }
    if let Some(report) = tasks.report(&id).data {
        for step in &report.steps {
            println!(
                "  {} {} [{:?}] {}",
                step.path, step.kind, step.outcome, step.note
            );
        }
    }
    browser.close();
    let at_payment = matches!(view.status, TaskStatus::Checkpoint { ref reason, .. } if reason.contains("payment"));
    println!(
        "{}",
        if at_payment {
            "PASS stopped at payment"
        } else {
            "FAIL did not stop at payment"
        }
    );
    if at_payment {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
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
