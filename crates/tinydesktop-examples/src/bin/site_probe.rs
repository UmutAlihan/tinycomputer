//! Opens each URL given on the command line in a real browser and prints what
//! the flow runtime would see there: the title, what is in front, the first
//! actionable controls, the repeated result cards, and whether a captcha or
//! login wall blocks the page. An argument `click=<name>` after a URL clicks
//! the first control whose name contains `<name>` and prints the page again.
//! It is the research step before pointing a live task at a site.
//!
//! Run it in the Docker lab, never on the host:
//! `scripts/docker-lab -- cargo run -p tinydesktop-examples --bin site_probe -- <url>...`

use std::sync::Arc;

use tinydesktop_browser::{AgentBrowser, Browser, BrowserSurface, SessionOptions};
use tinydesktop_bus::JevOperation;
use tinydesktop_core::human_needed;
use tinydesktop_core::surface::{Depth, Surface, result_groups};

/// How many controls to print per page.
const SHOWN: usize = 40;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let urls: Vec<String> = std::env::args().skip(1).collect();
    if urls.is_empty() {
        return Err("usage: site_probe <url>...".into());
    }
    let runtime = tokio::runtime::Runtime::new()?;
    let mut surface: Option<BrowserSurface> = None;
    for argument in &urls {
        if let Some(name) = argument.strip_prefix("click=") {
            if let Some(surface) = &surface {
                println!("=== click {name:?}");
                click(surface, name);
                show(surface);
            }
            continue;
        }
        if let Some(previous) = surface.take() {
            previous.close();
        }
        let fresh = BrowserSurface::new(
            Arc::new(Browser::new(Arc::new(AgentBrowser))),
            SessionOptions {
                executable: std::env::var("TINYDESKTOP_BROWSER_EXECUTABLE").ok(),
                ..SessionOptions::default()
            },
            runtime.handle().clone(),
        );
        println!("=== {argument}");
        let reply = fresh.navigate(argument);
        if reply.ok {
            show(&fresh);
        } else {
            println!("navigate failed: {:?}", reply.error);
        }
        surface = Some(fresh);
    }
    if let Some(surface) = surface {
        surface.close();
    }
    drop(runtime);
    Ok(())
}

/// Clicks the first control whose name contains `name`.
fn click(surface: &BrowserSurface, name: &str) {
    let Ok(screen) = surface.observe("browser", None, Depth::Full) else {
        println!("observe failed before the click");
        return;
    };
    let Some(target) = screen
        .candidates
        .iter()
        .find(|node| node.name.as_deref().is_some_and(|text| text.contains(name)))
    else {
        println!("no control named {name:?}");
        return;
    };
    let reply = surface.execute(JevOperation::Click, Some(target.clone()), None);
    println!("click {} -> ok {}", target.ref_id, reply.ok);
}

fn show(surface: &BrowserSurface) {
    let wait = std::env::var("PROBE_WAIT_SECS")
        .ok()
        .and_then(|secs| secs.parse().ok())
        .unwrap_or(8);
    std::thread::sleep(std::time::Duration::from_secs(wait));
    let screen = match surface.observe("browser", None, Depth::Full) {
        Ok(screen) => screen,
        Err(reply) => {
            println!("observe failed: {:?}", reply.error);
            return;
        }
    };
    println!(
        "title: {:?}\nin front: {}\ncontrols: {}  text lines: {}",
        screen.window,
        screen.surface,
        screen.candidates.len(),
        screen.context.len()
    );
    let texts: Vec<String> = screen
        .context
        .iter()
        .cloned()
        .chain(screen.candidates.iter().filter_map(|node| node.name.clone()))
        .collect();
    println!("human needed: {:?}", human_needed(&texts));
    for node in screen.candidates.iter().take(SHOWN) {
        println!(
            "  {} {} {:?} {}",
            node.ref_id,
            node.role,
            node.name.as_deref().unwrap_or(""),
            node.value.as_ref().map(ToString::to_string).unwrap_or_default()
        );
    }
    for line in screen.context.iter().take(15) {
        println!("  | {line}");
    }
    for group in result_groups(&screen).iter().take(12) {
        println!(
            "  [{}] {} -> {:?}",
            group.label,
            group.fields.join(" · "),
            group.primary.as_ref().map(|node| (&node.ref_id, &node.name))
        );
    }
}
