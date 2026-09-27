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

use std::sync::Arc;

use tinydesktop_browser::{AgentBrowser, Browser, BrowserSurface, SessionOptions};
use tinydesktop_core::surface::{Depth, Screen, Surface, result_groups};
use tinydesktop_core::{Criterion, FieldHint, Record, payment_evidence, rank};

type Checks = Vec<(&'static str, bool, String)>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base = std::env::var("TINYDESKTOP_FIXTURE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8765".to_owned());
    let runtime = tokio::runtime::Runtime::new()?;
    let surface = BrowserSurface::new(
        Arc::new(Browser::new(Arc::new(AgentBrowser))),
        SessionOptions {
            executable: std::env::var("TINYDESKTOP_BROWSER_EXECUTABLE").ok(),
            ..SessionOptions::default()
        },
        runtime.handle().clone(),
    );
    let mut checks = Checks::new();
    search(&surface, &base, &mut checks);
    results(&surface, &base, &mut checks);
    payment(&surface, &base, &mut checks);
    surface.close();
    drop(runtime);

    let failed = checks.iter().filter(|(_, passed, _)| !passed).count();
    for (name, passed, detail) in &checks {
        println!("{} {name}: {detail}", if *passed { "PASS" } else { "FAIL" });
    }
    if failed == 0 {
        println!("all checks passed");
        Ok(())
    } else {
        Err(format!("{failed} of {} checks failed", checks.len()).into())
    }
}

fn search(surface: &BrowserSurface, base: &str, checks: &mut Checks) {
    match open(surface, &format!("{base}/index.html")) {
        Ok(screen) => {
            checks.push((
                "cookie dialog in front",
                screen.surface == "sheet",
                format!("surface {:?}", screen.surface),
            ));
            checks.push((
                "search form fields",
                ["From", "To", "Departure date"].iter().all(|name| {
                    screen
                        .candidates
                        .iter()
                        .any(|node| node.name.as_deref() == Some(name))
                }),
                format!("{} controls", screen.candidates.len()),
            ));
        }
        Err(error) => checks.push(("search page", false, error)),
    }
}

fn results(surface: &BrowserSurface, base: &str, checks: &mut Checks) {
    let screen = match open(
        surface,
        &format!("{base}/results.html?from=Delhi&to=Srinagar&date=14%20October"),
    ) {
        Ok(screen) => screen,
        Err(error) => return checks.push(("results page", false, error)),
    };
    let groups = result_groups(&screen);
    checks.push((
        "four result cards",
        groups.len() == 4,
        format!("{} cards", groups.len()),
    ));
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
    let best = rank(&records, Criterion::LowestPrice).and_then(|order| order.first().copied());
    let picked = best
        .map(|index| groups[index].fields.join(" · "))
        .unwrap_or_default();
    checks.push((
        "cheapest is IndiGo at ₹6,840",
        picked.contains("IndiGo") && picked.contains("6,840"),
        picked,
    ));
    let opener = best.and_then(|index| groups[index].primary.clone());
    checks.push((
        "the card opens with Select",
        opener.as_ref().and_then(|node| node.name.as_deref()) == Some("Select"),
        format!("{:?}", opener.map(|node| node.name)),
    ));
}

fn payment(surface: &BrowserSurface, base: &str, checks: &mut Checks) {
    let url = format!("{base}/payment.html");
    let screen = match open(surface, &url) {
        Ok(screen) => screen,
        Err(error) => return checks.push(("payment page", false, error)),
    };
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
    let evidence = payment_evidence(&url, &fields, &controls);
    checks.push((
        "payment page detected",
        evidence.is_some(),
        format!("{evidence:?}"),
    ));
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
