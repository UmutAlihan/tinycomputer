//! Runs a whole booking task on the travel fixture through the bus, with
//! live Jev: search, pick the cheapest flight, fill the traveller form
//! (answering the phone number it asks for with `ContinueTask`), skip the
//! upsell, and stop at payment. It loads the built module through the
//! `TinyBus` loader and uses nothing but its bus members.
//!
//! Needs `OPENROUTER_API_KEY` for Jev. Run it in the Docker lab, never on the
//! host: `scripts/docker-lab -- crates/tinycomputer-examples/fixtures/run task_fixture`.
//! Set `TINYCOMPUTER_FLOW_STRATEGY=wide` to run it with the wide strategy,
//! and `TINYCOMPUTER_BROWSER_PERCEPTION=tree` to read pages by the tree.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::json;
use tinycomputer_bus::agent::TaskStatus;
use tinycomputer_bus::agent::{StartTaskRequest, SurfaceKind, TaskBudget, TaskConstraints};
use tinycomputer_examples::host::{Host, LabError, jev_config, module_path, openrouter_key};
use tinycomputer_examples::task::{conclude, follow};

#[tokio::main]
async fn main() -> Result<(), LabError> {
    let base = std::env::var("TINYCOMPUTER_FIXTURE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8765".to_owned());
    let mut browser = serde_json::Map::new();
    if let Ok(executable) = std::env::var("TINYCOMPUTER_BROWSER_EXECUTABLE") {
        browser.insert("executable".to_owned(), json!(executable));
    }
    if let Ok(perception) = std::env::var("TINYCOMPUTER_BROWSER_PERCEPTION") {
        browser.insert(
            "perception".to_owned(),
            json!(perception.trim().to_ascii_lowercase()),
        );
    }
    let config = json!({
        "jev": jev_config(openrouter_key()?, None)?,
        "cursor": "off",
        "browser": browser,
    });
    let host = Host::load(&module_path(), config).await?;

    let view = host.start_task(&request(&base)?).await?;
    let answers = BTreeMap::from([("phone".to_owned(), "+91 98765 43210".to_owned())]);
    let view = follow(&host, view, &answers, Duration::from_secs(20 * 60)).await?;
    conclude(&host, &view, &PathBuf::from("target/task-fixture")).await?;
    host.shutdown();
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
        constraints: TaskConstraints {
            surfaces: vec![SurfaceKind::Browser],
            ..TaskConstraints::default()
        },
        trace: true,
        budget: TaskBudget {
            strategy: tinycomputer_examples::flow_strategy_from_env(),
            deliberation: tinycomputer_examples::flow_deliberation_from_env(),
            ..TaskBudget::default()
        },
        ..StartTaskRequest::default()
    })
}
