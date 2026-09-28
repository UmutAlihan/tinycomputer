//! Runs a plain-language task end to end against real websites or desktop
//! applications, the way an outside agent would: it loads the built module
//! through the `TinyBus` loader and uses nothing but its bus members. The
//! module's planner writes the flow (`PlanTask`), the task runs with live
//! Jev (`StartTask`), and the run is followed until it stops (`AwaitTask`);
//! the report comes from `TaskReport` and the final screenshot from
//! `BrowserScreenshot` and `BrowserReadOutput`. Payment is never entered: a booking ends at the payment
//! checkpoint, and a task that only reads ends `done` with its records.
//!
//! Inputs, all read from the environment:
//!
//! - `TINYCOMPUTER_MODULE` — the attested module, as `scripts/build-module`
//!   prints it (default `$CARGO_TARGET_DIR/lab/`, or `target/lab/`).
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
//! - `TINYCOMPUTER_RESCUE_MODEL` — optional: the `OpenRouter` model that
//!   rescues them (`openai/gpt-6-luna` by default).
//! - `TINYCOMPUTER_DECISIONS` — optional: `sage` makes Levanto Sage take
//!   every decision in place of Jev, with `SAGE_API_KEY`, through the
//!   module's `jev` configuration; `SAGE_FAST=1` scores each choice in one
//!   pass. The planner and the rescuer still use `OPENROUTER_API_KEY`.
//! - `TINYCOMPUTER_BROWSER_EXECUTABLE`, `TINYCOMPUTER_BROWSER_USER_AGENT`, and
//!   `TINYCOMPUTER_BROWSER_ARGS` (space-separated) — how the browser
//!   launches, and `TINYCOMPUTER_BROWSER_PERCEPTION` (`sight` or `tree`) how
//!   pages are read; all passed as the module's `browser` configuration.
//! - `TASK_CURSOR` — optional: the agent's on-screen cursor pace (`off`,
//!   `brisk`, `natural`, `calm`; default `natural`). It is drawn by the
//!   `tinycomputer-cursor-overlay` helper, which the module finds beside
//!   itself, over a browser window on this screen — an attached Chrome, or a
//!   headed one.
//! - `TINYCOMPUTER_BROWSER_ENDPOINT` — attach to a running Chrome instead
//!   (`http://127.0.0.1:9222`); booking sites turn away a fresh headless
//!   browser but serve a person's own. Closing the run only disconnects.
//!
//! Launching a browser runs in the Docker lab, never on the host:
//! `scripts/docker-lab -- crates/tinycomputer-examples/tasks/run kashmir`.
//! Attaching to your own Chrome runs on the host, since that is where it is.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use tinycomputer_bus::Flow;
use tinycomputer_bus::agent::{
    PlanTaskRequest, StartTaskRequest, SurfaceKind, TaskBudget, TaskConstraints, TaskOutput,
};
use tinycomputer_examples::host::{Host, LabError, jev_config, module_path, openrouter_key};
use tinycomputer_examples::task::{conclude, follow, passed};

#[tokio::main]
async fn main() -> Result<(), LabError> {
    let task = std::fs::read_to_string(env("TASK_FILE")?)?;
    let (facts, secret_facts) = read_facts(&std::fs::read_to_string(env("FACTS_FILE")?)?)?;
    let out =
        PathBuf::from(std::env::var("TASK_OUT").unwrap_or_else(|_| "target/task-live".into()));
    std::fs::create_dir_all(&out)?;
    let output: Option<TaskOutput> = match std::env::var("OUTPUT_FILE") {
        Ok(path) => Some(serde_json::from_str(&std::fs::read_to_string(path)?)?),
        Err(_) => None,
    };
    let kind = surface_kind()?;

    let host = Host::load(&module_path(), module_config()?).await?;
    let flow = match std::env::var("FLOW_FILE") {
        Ok(path) => serde_json::from_str(&std::fs::read_to_string(path)?)?,
        Err(_) => plan(&host, &task, &facts, &secret_facts, kind, &out).await?,
    };
    // The task travels with the flow, so every Jev question is briefed on it.
    let view = host
        .start_task(&StartTaskRequest {
            task: Some(task.clone()),
            flow: Some(flow),
            facts,
            secret_facts,
            constraints: TaskConstraints {
                surfaces: vec![kind],
                browser_endpoint: std::env::var("TINYCOMPUTER_BROWSER_ENDPOINT").ok(),
                ..TaskConstraints::default()
            },
            budget: TaskBudget {
                max_actions: Some(200),
                max_model_calls: Some(5000),
                strategy: tinycomputer_examples::flow_strategy_from_env(),
                deliberation: tinycomputer_examples::flow_deliberation_from_env(),
                max_rescues: std::env::var("TASK_RESCUES")
                    .ok()
                    .and_then(|value| value.trim().parse().ok()),
                ..TaskBudget::default()
            },
            trace: true,
            output,
            ..StartTaskRequest::default()
        })
        .await?;
    let limit = Duration::from_secs(
        60 * std::env::var("TASK_MAX_MINUTES")
            .ok()
            .and_then(|minutes| minutes.parse().ok())
            .unwrap_or(20),
    );
    let view = follow(&host, view, &BTreeMap::new(), limit).await?;
    conclude(&host, &view, &out).await?;
    host.shutdown();
    if passed(&view.status) {
        println!(
            "PASS finished or stopped at payment; artifacts in {}",
            out.display()
        );
        Ok(())
    } else {
        Err("FAIL the task neither finished nor reached the payment checkpoint".into())
    }
}

/// The module's private configuration: Jev, the planner (which also brings
/// the rescuer and the output shaper), the cursor, and how browsers launch.
fn module_config() -> Result<Value, LabError> {
    let key = openrouter_key()?;
    let optional = |name: &str| std::env::var(name).ok();
    let mut browser = serde_json::Map::new();
    for (field, variable) in [
        ("executable", "TINYCOMPUTER_BROWSER_EXECUTABLE"),
        ("user_agent", "TINYCOMPUTER_BROWSER_USER_AGENT"),
    ] {
        if let Some(value) = optional(variable).filter(|value| !value.trim().is_empty()) {
            browser.insert(field.to_owned(), json!(value.trim()));
        }
    }
    if let Some(perception) = optional("TINYCOMPUTER_BROWSER_PERCEPTION") {
        let perception = perception.trim().to_ascii_lowercase();
        if !perception.is_empty() {
            browser.insert("perception".to_owned(), json!(perception));
        }
    }
    if let Some(args) = optional("TINYCOMPUTER_BROWSER_ARGS") {
        browser.insert(
            "args".to_owned(),
            json!(args.split_whitespace().collect::<Vec<_>>()),
        );
    }
    Ok(json!({
        "jev": decisions(&key)?,
        "planner": {
            "api_key": key,
            "model": optional("TINYCOMPUTER_PLANNER_MODEL"),
            "rescue_model": optional("TINYCOMPUTER_RESCUE_MODEL"),
            "output_model": optional("TINYCOMPUTER_OUTPUT_MODEL"),
        },
        "cursor": optional("TASK_CURSOR").unwrap_or_else(|| "natural".to_owned()),
        "browser": browser,
    }))
}

/// Who takes the flow's decisions, as the module's `jev` configuration:
/// Levanto Sage when `TINYCOMPUTER_DECISIONS` is `sage`, else Jev on
/// `OpenRouter` with `key`.
fn decisions(key: &str) -> Result<Value, LabError> {
    if std::env::var("TINYCOMPUTER_DECISIONS").as_deref() == Ok("sage") {
        let sage = std::env::var("SAGE_API_KEY")
            .map_err(|_| std::io::Error::other("TINYCOMPUTER_DECISIONS=sage needs SAGE_API_KEY"))?;
        let fast = std::env::var("SAGE_FAST").is_ok_and(|value| value == "1");
        return Ok(json!({"api_key": sage, "provider": "sage", "fast": fast}));
    }
    jev_config(key.to_owned(), None)
}

/// The surface the task runs on, from `TASK_SURFACE`.
fn surface_kind() -> Result<SurfaceKind, LabError> {
    match std::env::var("TASK_SURFACE").as_deref() {
        Err(_) | Ok("browser") => Ok(SurfaceKind::Browser),
        Ok("desktop") => Ok(SurfaceKind::Desktop),
        Ok(other) => {
            Err(format!("TASK_SURFACE must be `browser` or `desktop`, not `{other}`").into())
        }
    }
}

/// The facts file's values by name, and the names it marks secret.
fn read_facts(text: &str) -> Result<(BTreeMap<String, String>, Vec<String>), LabError> {
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

/// Asks the planner for a flow over the bus, prints it, and keeps a copy
/// beside the report.
async fn plan(
    host: &Host,
    task: &str,
    facts: &BTreeMap<String, String>,
    secret_facts: &[String],
    kind: SurfaceKind,
    out: &std::path::Path,
) -> Result<Flow, LabError> {
    let plan = host
        .plan_task(&PlanTaskRequest {
            task: task.to_owned(),
            fact_names: facts.keys().cloned().collect(),
            secret_facts: secret_facts.to_vec(),
            surfaces: vec![kind],
        })
        .await?;
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
