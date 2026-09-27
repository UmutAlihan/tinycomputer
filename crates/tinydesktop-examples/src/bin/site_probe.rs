//! Opens each URL given on the command line in a real browser and prints what
//! the flow runtime would see there: the title, what is in front, the first
//! actionable controls, the repeated result cards, and whether a captcha or
//! login wall blocks the page. An argument `click=<name>` after a URL clicks
//! the first control whose name contains `<name>` and prints the page again;
//! `type=<text>` types it with key presses into the focused field — only once
//! that field is verified to take text, and after naming it on stderr. The
//! typed text is never printed, and neither is any control's value.
//! `mouse=<x>,<y>` clicks at that point.
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
use tinydesktop_core::surface::{Candidate, Depth, Surface, result_groups};
use tinydesktop_core::{human_needed, screen_payment_evidence};

/// How many controls to print per page.
const SHOWN: usize = 400;

/// Describes the focused element when it takes typed text — the same test
/// `BrowserSurface` applies before typing without a target — and is `null`
/// otherwise. It names the field; it never reads its value.
const FOCUSED_FIELD: &str = r"(() => {
  const element = document.activeElement;
  if (!element) return null;
  const tag = (element.tagName || '').toLowerCase();
  const role = (element.getAttribute('role') || '').toLowerCase();
  const editable = tag === 'input' || tag === 'textarea' || element.isContentEditable
    || ['combobox', 'searchbox', 'textbox'].includes(role);
  if (!editable) return null;
  const type = element.getAttribute('type');
  const label = element.getAttribute('aria-label') || element.getAttribute('name')
    || element.getAttribute('placeholder') || element.id || '';
  return tag + (type ? '[type=' + type + ']' : '') + (label ? ' ' + JSON.stringify(label) : '');
})()";

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

    /// Runs one raw engine command on the open session.
    fn command(
        &self,
        surface: &BrowserSurface,
        command: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let session = surface
            .session()
            .ok_or_else(|| "no browser session is open".to_owned())?;
        self.runtime
            .block_on(self.browser.command(&session, command))
            .map_err(|error| error.to_string())
    }

    /// Clicks the page at `x,y` with real mouse events, stopping at the
    /// first event the browser refuses.
    fn mouse(&self, surface: &BrowserSurface, point: &str) {
        let (x, y) = match parse_point(point) {
            Ok(point) => point,
            Err(error) => {
                println!("=== mouse rejected: {error}");
                return;
            }
        };
        for event in ["mouseMoved", "mousePressed", "mouseReleased"] {
            let pressed = event != "mouseMoved";
            let sent = self.command(
                surface,
                serde_json::json!({"action": "mouse", "eventType": event, "x": x, "y": y,
                    "button": if pressed { "left" } else { "none" },
                    "clickCount": i32::from(pressed)}),
            );
            if let Err(error) = sent {
                println!("=== mouse {point} failed at {event}: {error}");
                return;
            }
        }
        println!("=== mouse {point}");
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    /// Types `text` with key presses into the focused field, once that field
    /// is verified to take typed text. The field is named on stderr before
    /// anything is typed; the text itself is never printed.
    fn type_keys(&self, surface: &BrowserSurface, text: &str) {
        let focus = self.command(
            surface,
            serde_json::json!({"action": "evaluate", "script": FOCUSED_FIELD}),
        );
        let field = match focus.map(|data| editable_focus(&data)) {
            Ok(Some(field)) => field,
            Ok(None) => {
                println!("=== type refused: no editable field has focus");
                return;
            }
            Err(error) => {
                println!("=== type refused: the focus check failed: {error}");
                return;
            }
        };
        eprintln!("=== typing into {field}");
        let typed = self.command(
            surface,
            serde_json::json!({"action": "keyboard", "subaction": "type", "text": text}),
        );
        if let Err(error) = typed {
            println!("=== type failed: {error}");
            return;
        }
        println!("=== type -> ok");
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
            match snapshot {
                Ok(snapshot) => {
                    for line in snapshot
                        .tree
                        .lines()
                        .filter(|line| line.to_lowercase().contains(&pattern.to_lowercase()))
                    {
                        println!("  raw: {line}");
                    }
                }
                Err(error) => println!("=== PROBE_GREP failed: {error}"),
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
        println!("{}", control_line(node));
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

/// The `x,y` of a `mouse=` argument: two finite numbers, or an error naming
/// what is wrong, so a typo never becomes a click somewhere real.
fn parse_point(point: &str) -> Result<(f64, f64), String> {
    let (x, y) = point
        .split_once(',')
        .ok_or_else(|| format!("expected x,y, got {point:?}"))?;
    let coordinate = |text: &str| {
        text.trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("{text:?} is not a coordinate"))
    };
    Ok((coordinate(x)?, coordinate(y)?))
}

/// One control as the page dump prints it. The value is left out: a field
/// can hold a password or card number `type=` entered moments earlier.
fn control_line(node: &Candidate) -> String {
    format!(
        "  {} {} {:?}",
        node.ref_id,
        node.role,
        node.name.as_deref().unwrap_or("")
    )
}

/// The focused element [`FOCUSED_FIELD`] described, when it takes typed text.
fn editable_focus(data: &serde_json::Value) -> Option<String> {
    data.get("result")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tinydesktop_core::surface::Candidate;

    use super::{control_line, editable_focus, parse_point};

    #[test]
    fn parses_a_point_of_two_numbers() {
        assert_eq!(parse_point("120,48.5"), Ok((120.0, 48.5)));
        assert_eq!(parse_point(" 120 , 48 "), Ok((120.0, 48.0)));
    }

    #[test]
    fn rejects_a_point_that_is_not_two_numbers() {
        for point in ["120", "x,48", "120,y", ",48", "120,", "NaN,1", "1,inf"] {
            assert!(parse_point(point).is_err(), "{point:?} parsed");
        }
    }

    #[test]
    fn control_line_leaves_the_value_out() {
        let node = Candidate {
            ref_id: "e7".to_owned(),
            role: "textbox".to_owned(),
            name: Some("Card number".to_owned()),
            value: Some(json!("4111111111111111")),
            ..Candidate::default()
        };
        let line = control_line(&node);
        assert_eq!(line, r#"  e7 textbox "Card number""#);
        assert!(!line.contains("4111"));
    }

    #[test]
    fn names_the_focused_field_only_when_it_is_editable() {
        assert_eq!(
            editable_focus(&json!({"result": "input[type=password] \"Password\""})),
            Some("input[type=password] \"Password\"".to_owned())
        );
        assert_eq!(editable_focus(&json!({"result": null})), None);
        assert_eq!(editable_focus(&json!({"result": false})), None);
        assert_eq!(editable_focus(&json!({})), None);
    }
}
