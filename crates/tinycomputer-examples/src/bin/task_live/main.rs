//! Runs a plain-language task end to end against real websites or desktop
//! applications, the way an outside agent would through the task API: the
//! in-module planner writes the flow (`PlanTask`), the task controller runs
//! it with live Jev (`StartTask`), and the run is followed until it stops
//! (`AwaitTask`). Payment is never entered: a booking ends at the payment
//! checkpoint, and a task that only reads ends `done` with its records.
//!
//! Inputs, all read from the environment:
//!
//! - `OPENROUTER_API_KEY` — Jev and the planner.
//! - `TASK_FILE` — the task in plain language.
//! - `FACTS_FILE` — a JSON object of facts for the task, by name. A value is
//!   a string, or `{"value": "...", "secret": true}` to keep it secret; a
//!   card number or passport number is secret anyway.
//! - `FLOW_FILE` — optional: run this flow instead of planning one.
//! - `TASK_OUT` — optional: where the plan, report, and final screenshot go
//!   (default `target/task-live`).
//! - `OUTPUT_FILE` — optional: a JSON `TaskOutput` (`instructions` and a
//!   `schema`) asking for the answer in a fixed shape; the result is written
//!   to `result.json`.
//! - `TINYCOMPUTER_OUTPUT_MODEL` — optional: the `OpenRouter` model that
//!   shapes it (`openai/gpt-6-luna` by default).
//! - `TASK_SURFACE` — optional: `browser` (default) or `desktop`, the
//!   applications on this Mac through the accessibility tree. A desktop task
//!   runs on the host, in a shell that has the Accessibility permission.
//! - `TINYCOMPUTER_FLOW_STRATEGY` — optional: `narrow` (default) or `wide`.
//! - `TINYCOMPUTER_FLOW_DELIBERATION` — optional: `deep` (default),
//!   `standard`, or `off`.
//! - `TASK_MAX_MINUTES` — optional: cancel the task after this long (20).
//! - `TASK_RESCUES` — optional: how many failed steps the reasoning model
//!   may rescue (0 to 5, default 5; 0 turns rescues off).
//! - `TINYCOMPUTER_DECISIONS` — optional: `sage` makes Levanto Sage take
//!   every decision in place of Jev, with `SAGE_API_KEY`; `SAGE_FAST=1`
//!   scores each choice in one pass. The planner and the rescuer still use
//!   `OPENROUTER_API_KEY`.
//! - `TINYCOMPUTER_RESCUE_MODEL` — optional: the `OpenRouter` model that
//!   rescues them (`openai/gpt-6-luna` by default).
//! - `TINYCOMPUTER_BROWSER_EXECUTABLE`, `TINYCOMPUTER_BROWSER_USER_AGENT`, and
//!   `TINYCOMPUTER_BROWSER_ARGS` (space-separated) — how the browser launches.
//! - `TASK_CURSOR` — optional: the agent's on-screen cursor pace (`off`,
//!   `brisk`, `natural`, `calm`; default `natural`). It is drawn by the
//!   `tinycomputer-cursor-overlay` helper, found beside this binary once built
//!   with `cargo build -p tinycomputer-cursor --features overlay`, over a
//!   browser window on this screen — an attached Chrome, or a headed one.
//! - `TINYCOMPUTER_BROWSER_ENDPOINT` — attach to a running Chrome instead
//!   (`http://127.0.0.1:9222`); booking sites turn away a fresh headless
//!   browser but serve a person's own. Closing the run only disconnects.
//!
//! Launching a browser runs in the Docker lab, never on the host:
//! `scripts/docker-lab -- crates/tinycomputer-examples/tasks/run kashmir`.
//! Attaching to your own Chrome runs on the host, since that is where it is.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use tinycomputer::Desktop;
use tinycomputer_browser::{
    AgentBrowser, Browser, BrowserSurface, CursorPace, ScreenCursor, SessionOptions,
};
use tinycomputer_bus::agent::{
    AwaitTaskRequest, PlanTaskRequest, StartTaskRequest, SurfaceKind, TaskBudget, TaskConstraints,
    TaskId, TaskOutput, TaskStatus, TaskView,
};
use tinycomputer_bus::{Flow, JevConfig, RunFlowRequest};
use tinycomputer_engine::{
    FlowFuture, FlowRunner, JevRuntime, PlannerConfig, Tasks, TextFuture, Workspace, open_router,
    open_router_rescuer, open_router_shaper,
};

type Failure = Box<dyn std::error::Error>;

struct Live {
    workspace: Workspace<Desktop, BrowserSurface>,
    jev: JevRuntime,
}

impl FlowRunner for Live {
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
async fn main() -> Result<(), Failure> {
    let key = std::env::var("OPENROUTER_API_KEY")
        .map_err(|_| "OPENROUTER_API_KEY is not set; Jev and the planner cannot run")?;
    let task = std::fs::read_to_string(env("TASK_FILE")?)?;
    let (facts, secret_facts) = read_facts(&std::fs::read_to_string(env("FACTS_FILE")?)?)?;
    let out =
        PathBuf::from(std::env::var("TASK_OUT").unwrap_or_else(|_| "target/task-live".into()));
    std::fs::create_dir_all(&out)?;

    let jev = decisions(&key)?;
    let planner: PlannerConfig = serde_json::from_value(json!({
        "api_key": key,
        "model": std::env::var("TINYCOMPUTER_PLANNER_MODEL").ok(),
        "rescue_model": std::env::var("TINYCOMPUTER_RESCUE_MODEL").ok(),
        "output_model": std::env::var("TINYCOMPUTER_OUTPUT_MODEL").ok(),
    }))?;
    let output: Option<TaskOutput> = match std::env::var("OUTPUT_FILE") {
        Ok(path) => Some(serde_json::from_str(&std::fs::read_to_string(path)?)?),
        Err(_) => None,
    };
    let kind = surface_kind()?;
    let cursor = Arc::new(cursor()?);
    let browser = Arc::new(Browser::new(Arc::new(AgentBrowser)));
    let surface = (kind == SurfaceKind::Browser).then(|| {
        BrowserSurface::new(
            browser.clone(),
            SessionOptions {
                endpoint: std::env::var("TINYCOMPUTER_BROWSER_ENDPOINT").ok(),
                executable: std::env::var("TINYCOMPUTER_BROWSER_EXECUTABLE").ok(),
                user_agent: std::env::var("TINYCOMPUTER_BROWSER_USER_AGENT").ok(),
                args: std::env::var("TINYCOMPUTER_BROWSER_ARGS")
                    .map(|args| args.split_whitespace().map(str::to_owned).collect())
                    .unwrap_or_default(),
                ..SessionOptions::default()
            },
            tokio::runtime::Handle::current(),
        )
        .with_cursor(cursor.clone())
        .with_perception(tinycomputer_examples::perception_from_env())
    });
    let desktop = (kind == SurfaceKind::Desktop).then(|| Desktop::new().with_cursor(cursor));
    let tasks = Tasks::new(Arc::new(Live {
        workspace: Workspace::new(desktop, surface.clone()),
        jev,
    }))
    .with_planner(open_router(&planner)?)
    .with_rescuer(open_router_rescuer(&planner)?)
    .with_shaper(open_router_shaper(&planner)?);

    let flow = match std::env::var("FLOW_FILE") {
        Ok(path) => serde_json::from_str(&std::fs::read_to_string(path)?)?,
        Err(_) => plan(&tasks, &task, &facts, &secret_facts, kind, &out).await?,
    };
    // The task travels with the flow, so every Jev question is briefed on it.
    let started = tasks.start(&StartTaskRequest {
        task: Some(task.clone()),
        flow: Some(flow),
        facts,
        secret_facts: secret_facts.clone(),
        constraints: TaskConstraints {
            surfaces: vec![kind],
            ..TaskConstraints::default()
        },
        budget: TaskBudget {
            max_actions: Some(200),
            max_model_calls: Some(5000),
            max_elapsed_ms: None,
            votes: None,
            strategy: tinycomputer_examples::flow_strategy_from_env(),
            deliberation: tinycomputer_examples::flow_deliberation_from_env(),
            max_rescues: std::env::var("TASK_RESCUES")
                .ok()
                .and_then(|value| value.trim().parse().ok()),
        },
        trace: true,
        output,
        ..StartTaskRequest::default()
    });
    let view = started
        .data
        .ok_or_else(|| format!("the task did not start: {:?}", started.error))?;
    let id = view.id.clone();
    let view = follow(&tasks, view).await?;
    conclude(&tasks, &id, &view, surface.as_ref(), &browser, &out).await
}

/// Saves the report, the final screenshot, and what the task returned, and
/// says whether it passed: finished, or stopped at payment.
async fn conclude(
    tasks: &Tasks,
    id: &TaskId,
    view: &TaskView,
    surface: Option<&BrowserSurface>,
    browser: &Browser,
    out: &std::path::Path,
) -> Result<(), Failure> {
    if let Some(report) = tasks.report(id).data {
        for step in &report.steps {
            println!(
                "  {} {} [{:?}] {}",
                step.path, step.kind, step.outcome, step.note
            );
        }
        std::fs::write(
            out.join("report.json"),
            serde_json::to_string_pretty(&report)?,
        )?;
    }
    if let Some(surface) = surface {
        if let Some(session) = surface.session() {
            let path = out.join("final.png");
            let shot = browser
                .command(
                    &session,
                    json!({"action": "screenshot", "path": path.to_string_lossy()}),
                )
                .await;
            println!("screenshot: {} ({})", path.display(), shot.is_ok());
        }
        surface.close();
    }
    println!("final: [{}] {}", state(&view.status), view.summary);
    match view.status {
        TaskStatus::Checkpoint { ref reason, .. } if reason.contains("payment") => {
            println!("PASS stopped at payment");
            Ok(())
        }
        TaskStatus::Done {
            ref records,
            ref result,
            ..
        } => {
            std::fs::write(
                out.join("records.json"),
                serde_json::to_string_pretty(records)?,
            )?;
            if let Some(result) = result {
                std::fs::write(
                    out.join("result.json"),
                    serde_json::to_string_pretty(result)?,
                )?;
            }
            println!("PASS finished; records and any result in {}", out.display());
            Ok(())
        }
        _ => Err("FAIL the task neither finished nor reached the payment checkpoint".into()),
    }
}

/// The surface the task runs on, from `TASK_SURFACE`.
fn surface_kind() -> Result<SurfaceKind, Failure> {
    match std::env::var("TASK_SURFACE").as_deref() {
        Err(_) | Ok("browser") => Ok(SurfaceKind::Browser),
        Ok("desktop") => Ok(SurfaceKind::Desktop),
        Ok(other) => {
            Err(format!("TASK_SURFACE must be `browser` or `desktop`, not `{other}`").into())
        }
    }
}

/// Who takes the flow's decisions: Levanto Sage when `TINYCOMPUTER_DECISIONS`
/// is `sage`, else Jev on `OpenRouter` with `key`.
fn decisions(key: &str) -> Result<JevRuntime, Failure> {
    if std::env::var("TINYCOMPUTER_DECISIONS").as_deref() == Ok("sage") {
        let sage = std::env::var("SAGE_API_KEY")
            .map_err(|_| "TINYCOMPUTER_DECISIONS=sage needs SAGE_API_KEY")?;
        let fast = std::env::var("SAGE_FAST").is_ok_and(|value| value == "1");
        return Ok(JevRuntime::sage(&sage, fast).map_err(|error| error.message)?);
    }
    let jev: JevConfig =
        serde_json::from_value(json!({"api_key": key, "provider": "open_router"}))?;
    Ok(JevRuntime::configure(&jev).map_err(|error| error.message)?)
}

/// The facts file's values by name, and the names it marks secret.
fn read_facts(text: &str) -> Result<(BTreeMap<String, String>, Vec<String>), Failure> {
    let raw: BTreeMap<String, serde_json::Value> = serde_json::from_str(text)?;
    let mut facts = BTreeMap::new();
    let mut secret = Vec::new();
    for (name, value) in raw {
        let text = match &value {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Object(fields) => {
                if fields.get("secret").and_then(serde_json::Value::as_bool) == Some(true) {
                    secret.push(name.clone());
                }
                fields
                    .get("value")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| format!("fact `{name}` has no string `value`"))?
                    .to_owned()
            }
            _ => return Err(format!("fact `{name}` must be a string or an object").into()),
        };
        facts.insert(name, text);
    }
    Ok((facts, secret))
}

fn env(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|_| format!("{name} is not set"))
}

/// Asks the planner for a flow, prints it, and keeps a copy beside the report.
async fn plan(
    tasks: &Tasks,
    task: &str,
    facts: &BTreeMap<String, String>,
    secret_facts: &[String],
    kind: SurfaceKind,
    out: &std::path::Path,
) -> Result<Flow, Failure> {
    let reply = tasks
        .plan(&PlanTaskRequest {
            task: task.to_owned(),
            fact_names: facts.keys().cloned().collect(),
            secret_facts: secret_facts.to_vec(),
            surfaces: vec![kind],
        })
        .await;
    let plan = reply
        .data
        .ok_or_else(|| format!("the planner failed: {:?}", reply.error))?;
    let text = serde_json::to_string_pretty(&plan.flow)?;
    println!("plan:\n{text}");
    for note in &plan.notes {
        println!("  note: {note}");
    }
    for question in &plan.questions {
        println!("  question: {} ({})", question.name, question.why);
    }
    std::fs::write(out.join("plan.json"), &text)?;
    Ok(plan.flow)
}

/// Follows the task until it stops, cancelling it past the time limit.
async fn follow(tasks: &Tasks, mut view: TaskView) -> Result<TaskView, String> {
    let limit = Duration::from_secs(
        60 * std::env::var("TASK_MAX_MINUTES")
            .ok()
            .and_then(|minutes| minutes.parse().ok())
            .unwrap_or(20),
    );
    let started = Instant::now();
    let id = view.id.clone();
    let mut last = String::new();
    loop {
        let line = format!("[{}] {}", state(&view.status), view.summary);
        if line != last {
            println!("{line}");
            last = line;
        }
        if !matches!(view.status, TaskStatus::Running) {
            return Ok(view);
        }
        let Some(wait) = next_wait(started.elapsed(), limit) else {
            println!("time limit reached; cancelling");
            return tasks
                .cancel(&id)
                .data
                .ok_or_else(|| "the task could not be cancelled".to_owned());
        };
        let reply = tasks
            .await_task(AwaitTaskRequest {
                id: id.clone(),
                timeout_ms: u64::try_from(wait.as_millis()).unwrap_or(u64::MAX),
            })
            .await;
        view = reply
            .data
            .ok_or_else(|| format!("the task stopped answering: {:?}", reply.error))?;
    }
}

/// The longest one `AwaitTask` call blocks before the loop looks again.
const AWAIT_SLICE: Duration = Duration::from_secs(30);

/// How long the next `AwaitTask` may block: what is left of the time limit,
/// at most one slice, so a wait never carries the run past the limit. `None`
/// once the limit is spent.
fn next_wait(elapsed: Duration, limit: Duration) -> Option<Duration> {
    limit
        .checked_sub(elapsed)
        .filter(|left| !left.is_zero())
        .map(|left| left.min(AWAIT_SLICE))
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

/// The agent's cursor at the `TASK_CURSOR` pace.
fn cursor() -> Result<ScreenCursor, Failure> {
    let pace: CursorPace =
        std::env::var("TASK_CURSOR").map_or(Ok(CursorPace::default()), |pace| pace.parse())?;
    Ok(if pace.is_off() {
        ScreenCursor::off()
    } else {
        ScreenCursor::new(pace, None)
    })
}

#[cfg(test)]
mod main_tests;
