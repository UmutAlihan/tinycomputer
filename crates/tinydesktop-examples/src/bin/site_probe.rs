//! Opens each URL given on the command line in a real browser and prints what
//! the flow runtime would see there: the title, what is in front, the first
//! actionable controls, the repeated result cards, and whether a captcha or
//! login wall blocks the page. An argument `click=<name>` after a URL clicks
//! the first control whose name contains `<name>` and prints the page again;
//! `type=<text>` types it with key presses wherever the focus is.
//! It is the research step before pointing a live task at a site.
//! `PROBE_ENDPOINT` attaches to a running Chrome, where the URL `current`
//! reads the page it already shows.
//!
//! Run it in the Docker lab, never on the host:
//! `scripts/docker-lab -- cargo run -p tinydesktop-examples --bin site_probe -- <url>...`

use std::sync::Arc;

use tinydesktop_browser::{AgentBrowser, Browser, BrowserSurface, SessionOptions};
use tinydesktop_bus::JevOperation;
use tinydesktop_bus::browser::SnapshotRequest;
use tinydesktop_core::surface::{Depth, Surface, result_groups};
use tinydesktop_core::{human_needed, screen_payment_evidence};

/// How many controls to print per page.
const SHOWN: usize = 400;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let urls: Vec<String> = std::env::args().skip(1).collect();
    if urls.is_empty() {
        return Err("usage: site_probe <url>...".into());
    }
    let runtime = tokio::runtime::Runtime::new()?;
    let probe = Probe {
        browser: Arc::new(Browser::new(Arc::new(AgentBrowser))),
        runtime: &runtime,
    };
    let mut surface: Option<BrowserSurface> = None;
    for argument in &urls {
        if let Some(open) = &surface {
            if let Some(point) = argument.strip_prefix("mouse=") {
                probe.mouse(open, point);
                continue;
            }
            if let Some(text) = argument.strip_prefix("type=") {
                probe.type_keys(open, text);
                continue;
            }
            if let Some(name) = argument.strip_prefix("click=") {
                println!("=== click {name:?}");
                click(open, name);
                show(open);
                continue;
            }
        }
        if let Some(previous) = surface.take() {
            previous.close();
        }
        surface = Some(probe.open(argument));
    }
    if let Some(surface) = surface {
        probe.finish(&surface);
        surface.close();
    }
    drop(runtime);
    Ok(())
}

/// The browser and runtime every probe step shares.
struct Probe<'a> {
    browser: Arc<Browser>,
    runtime: &'a tokio::runtime::Runtime,
}

impl Probe<'_> {
    /// A fresh session showing `url`, or the page an attached browser
    /// already shows for `current`.
    fn open(&self, url: &str) -> BrowserSurface {
        let surface = BrowserSurface::new(
            self.browser.clone(),
            SessionOptions {
                executable: std::env::var("TINYDESKTOP_BROWSER_EXECUTABLE").ok(),
                user_agent: std::env::var("PROBE_USER_AGENT").ok(),
                endpoint: std::env::var("PROBE_ENDPOINT").ok(),
                args: std::env::var("PROBE_BROWSER_ARGS")
                    .map(|args| args.split_whitespace().map(str::to_owned).collect())
                    .unwrap_or_default(),
                ..SessionOptions::default()
            },
            self.runtime.handle().clone(),
        );
        println!("=== {url}");
        let reply = if url == "current" {
            tinydesktop_bus::DesktopResponse::ok("navigate", serde_json::json!({}))
        } else {
            surface.navigate(url)
        };
        if reply.ok {
            show(&surface);
        } else {
            println!("navigate failed: {:?}", reply.error);
        }
        surface
    }

    fn command(&self, surface: &BrowserSurface, command: serde_json::Value) -> bool {
        surface.session().is_some_and(|session| {
            self.runtime
                .block_on(self.browser.command(&session, command))
                .is_ok()
        })
    }

    /// Clicks the page at `x,y` with real mouse events.
    fn mouse(&self, surface: &BrowserSurface, point: &str) {
        let Some((x, y)) = point.split_once(',') else {
            return;
        };
        let (x, y) = (
            x.parse::<f64>().unwrap_or_default(),
            y.parse::<f64>().unwrap_or_default(),
        );
        for event in ["mouseMoved", "mousePressed", "mouseReleased"] {
            let pressed = event != "mouseMoved";
            self.command(
                surface,
                serde_json::json!({"action": "mouse", "eventType": event, "x": x, "y": y,
                    "button": if pressed { "left" } else { "none" },
                    "clickCount": i32::from(pressed)}),
            );
        }
        println!("=== mouse {point}");
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    /// Types `text` with key presses wherever the focus is.
    fn type_keys(&self, surface: &BrowserSurface, text: &str) {
        let typed = self.command(
            surface,
            serde_json::json!({"action": "keyboard", "subaction": "type", "text": text}),
        );
        println!("=== type {text:?} -> {typed}");
        std::thread::sleep(std::time::Duration::from_secs(2));
        show(surface);
    }

    /// Runs `PROBE_JS` and prints the raw snapshot lines matching
    /// `PROBE_GREP`, on the last page.
    fn finish(&self, surface: &BrowserSurface) {
        let Some(session) = surface.session() else {
            return;
        };
        if let Ok(script) = std::env::var("PROBE_JS") {
            let value = self.runtime.block_on(self.browser.command(
                &session,
                serde_json::json!({"action": "evaluate", "script": script}),
            ));
            println!("=== PROBE_JS -> {value:?}");
        }
        if let Ok(pattern) = std::env::var("PROBE_GREP") {
            let snapshot = self
                .runtime
                .block_on(self.browser.snapshot(&session, SnapshotRequest::default()));
            let tree = snapshot.map(|snapshot| snapshot.tree).unwrap_or_default();
            for line in tree
                .lines()
                .filter(|line| line.to_lowercase().contains(&pattern.to_lowercase()))
            {
                println!("  raw: {line}");
            }
        }
    }
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
    println!(
        "click {} -> ok {} {:?}",
        target.ref_id,
        reply.ok,
        reply.error.map(|error| error.message)
    );
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
        .chain(
            screen
                .candidates
                .iter()
                .filter_map(|node| node.name.clone()),
        )
        .collect();
    println!("human needed: {:?}", human_needed(&texts));
    println!("payment page: {:?}", screen_payment_evidence(&screen));
    for node in screen.candidates.iter().take(SHOWN) {
        println!(
            "  {} {} {:?} {}",
            node.ref_id,
            node.role,
            node.name.as_deref().unwrap_or(""),
            node.value
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default()
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
            group
                .primary
                .as_ref()
                .map(|node| (&node.ref_id, &node.name))
        );
    }
}
