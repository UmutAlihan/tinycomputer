//! Drives the travel fixture in a real browser and checks each layer the
//! unified agent relies on, without Jev: agent-browser linked in-process,
//! snapshot parsing into a screen, result grouping and exact ranking, and
//! payment detection.
//!
//! Run it in the Docker lab (`docs/docker-lab.md`), never on the host:
//!
//! ```sh
//! scripts/docker-lab -- bash -c '
//!   python3 -m http.server 8765 --directory crates/tinydesktop-examples/fixtures/travel &
//!   cargo run -p tinydesktop-examples --bin browser_fixture'
//! ```
//!
//! `TINYDESKTOP_FIXTURE_URL` overrides the fixture address, and
//! `TINYDESKTOP_BROWSER_EXECUTABLE` names the browser binary where discovery
//! would not find one (Playwright's arm64 Chromium, for one).

use std::process::ExitCode;
use std::sync::Arc;

use tinydesktop_browser::{AgentBrowser, Browser, BrowserSurface, SessionOptions};
use tinydesktop_core::surface::{Depth, Screen, Surface, result_groups};
use tinydesktop_core::{Criterion, FieldHint, Record, payment_evidence, rank};

fn main() -> ExitCode {
    let base = std::env::var("TINYDESKTOP_FIXTURE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8765".to_owned());
    let runtime = tokio::runtime::Runtime::new().expect("a Tokio runtime starts");
    let surface = BrowserSurface::new(
        Arc::new(Browser::new(Arc::new(AgentBrowser))),
        SessionOptions {
            executable: std::env::var("TINYDESKTOP_BROWSER_EXECUTABLE").ok(),
            ..SessionOptions::default()
        },
        runtime.handle().clone(),
    );
    let mut failures = 0;
    let mut check = |name: &str, passed: bool, detail: &str| {
        println!("{} {name}: {detail}", if passed { "PASS" } else { "FAIL" });
        if !passed {
            failures += 1;
        }
    };

    let search = open(&surface, &format!("{base}/index.html"));
    match &search {
        Ok(screen) => {
            check(
                "cookie dialog in front",
                screen.surface == "sheet",
                &format!("surface {:?}", screen.surface),
            );
            check(
                "search form fields",
                ["From", "To", "Departure date"].iter().all(|name| {
                    screen
                        .candidates
                        .iter()
                        .any(|node| node.name.as_deref() == Some(name))
                }),
                &format!("{} controls", screen.candidates.len()),
            );
        }
        Err(error) => check("search page", false, error),
    }

    match open(
        &surface,
        &format!("{base}/results.html?from=Delhi&to=Srinagar&date=14%20October"),
    ) {
        Ok(screen) => {
            let groups = result_groups(&screen);
            check(
                "four result cards",
                groups.len() == 4,
                &format!("{groups:?}"),
            );
            let records = groups
                .iter()
                .map(|group| Record {
                    fields: group
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(index, text)| (format!("field {index}"), text.clone()))
                        .collect(),
                })
                .collect::<Vec<_>>();
            let best =
                rank(&records, Criterion::LowestPrice).and_then(|order| order.first().copied());
            let picked = best
                .map(|index| groups[index].fields.join(" · "))
                .unwrap_or_default();
            check(
                "cheapest is IndiGo at ₹6,840",
                picked.contains("IndiGo") && picked.contains("6,840"),
                &picked,
            );
            let opener = best.and_then(|index| groups[index].primary.clone());
            check(
                "the card opens with Select",
                opener.as_ref().and_then(|node| node.name.as_deref()) == Some("Select"),
                &format!("{:?}", opener.map(|node| node.name)),
            );
        }
        Err(error) => check("results page", false, &error),
    }

    match open(&surface, &format!("{base}/payment.html")) {
        Ok(screen) => {
            let fields = screen
                .candidates
                .iter()
                .map(|node| FieldHint {
                    label: node.name.clone().unwrap_or_default(),
                    ..FieldHint::default()
                })
                .collect::<Vec<_>>();
            let controls = screen
                .candidates
                .iter()
                .filter_map(|node| node.name.as_deref())
                .collect::<Vec<_>>();
            let evidence = payment_evidence(&format!("{base}/payment.html"), &fields, &controls);
            check(
                "payment page detected",
                evidence.is_some(),
                &format!("{evidence:?}"),
            );
        }
        Err(error) => check("payment page", false, &error),
    }

    surface.close();
    drop(runtime);
    if failures == 0 {
        println!("all checks passed");
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn open(surface: &BrowserSurface, url: &str) -> Result<Screen, String> {
    let reply = surface.navigate(url);
    if !reply.ok {
        return Err(format!("{url}: {:?}", reply.error));
    }
    surface
        .observe("browser", None, Depth::Full)
        .map_err(|reply| format!("{url}: {:?}", reply.error))
}
