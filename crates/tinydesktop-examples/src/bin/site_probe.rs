//! Opens each URL given on the command line in a real browser and prints what
//! the flow runtime would see there: the title, what is in front, the first
//! actionable controls, and whether a captcha or login wall blocks the page.
//! It is the research step before pointing a live task at a site.
//!
//! Run it in the Docker lab, never on the host:
//! `scripts/docker-lab -- cargo run -p tinydesktop-examples --bin site_probe -- <url>...`

use std::sync::Arc;

use tinydesktop_browser::{AgentBrowser, Browser, BrowserSurface, SessionOptions};
use tinydesktop_core::human_needed;
use tinydesktop_core::surface::{Depth, Surface};

/// How many controls to print per page.
const SHOWN: usize = 40;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let urls: Vec<String> = std::env::args().skip(1).collect();
    if urls.is_empty() {
        return Err("usage: site_probe <url>...".into());
    }
    let runtime = tokio::runtime::Runtime::new()?;
    for url in &urls {
        let surface = BrowserSurface::new(
            Arc::new(Browser::new(Arc::new(AgentBrowser))),
            SessionOptions {
                executable: std::env::var("TINYDESKTOP_BROWSER_EXECUTABLE").ok(),
                ..SessionOptions::default()
            },
            runtime.handle().clone(),
        );
        println!("=== {url}");
        probe(&surface, url);
        surface.close();
    }
    drop(runtime);
    Ok(())
}

fn probe(surface: &BrowserSurface, url: &str) {
    let reply = surface.navigate(url);
    if !reply.ok {
        println!("navigate failed: {:?}", reply.error);
        return;
    }
    std::thread::sleep(std::time::Duration::from_secs(4));
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
}
