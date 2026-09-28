//! Tests for reading sight's reply: controls, text, and when the tree is
//! read instead.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::{Denoised, denoised, is_seen, screen, script, selector};

/// A booking widget as sight reads it: a cookie dialog in front, a field
/// named by the words above it, a city row a page marked `combobox`, and an
/// icon button.
fn reading() -> serde_json::Value {
    json!({
        "ok": true,
        "title": "Book a flight",
        "surface": "sheet",
        "unreachable": 0,
        "nodes": [
            {"text": "We use cookies", "path": ["dialog \"cookieconsent\""]},
            {"id": "1", "role": "button", "name": "Accept All", "description": "",
             "value": "", "states": [], "box": [10, 700, 120, 40],
             "path": ["dialog \"cookieconsent\""]},
            {"text": "To", "path": ["main"]},
            {"id": "2", "role": "textbox", "name": "To", "description": "Destination",
             "value": "Srin", "states": ["required"], "box": [10, 100, 300, 40],
             "path": ["main", "form"]},
            {"id": "3", "role": "button", "name": "Mumbai BOM", "description": "",
             "value": "", "states": ["covered"], "box": [10, 150, 300, 40],
             "path": ["main", "form"]},
            {"id": "4", "role": "checkbox", "name": "Return trip", "description": "",
             "value": "", "states": ["checked"], "box": [10, 200, 20, 20], "path": []},
            {"id": "5", "role": "button", "name": "close", "description": "an icon",
             "value": "", "states": ["offscreen"], "box": [], "path": []},
            {"text": "We use cookies", "path": ["main"]}
        ]
    })
}

#[test]
fn a_reading_becomes_controls_text_and_context_in_page_order() {
    let screen = screen(&reading()).unwrap();
    assert_eq!(screen.window.as_deref(), Some("Book a flight"));
    assert_eq!(screen.surface, "sheet");
    let refs = screen
        .candidates
        .iter()
        .map(|node| node.ref_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(refs, ["seen:1", "seen:2", "seen:3", "seen:4", "seen:5"]);
    assert_eq!(
        screen.context,
        ["We use cookies", "To"],
        "repeats kept once"
    );
    assert_eq!(screen.text_nodes.len(), 3);
    assert_eq!(screen.text_nodes[0].role, "text");
    assert_eq!(
        screen.text_nodes[1].order, 2,
        "text keeps its place among controls"
    );

    let field = &screen.candidates[1];
    assert_eq!(field.name.as_deref(), Some("To"));
    assert_eq!(field.description.as_deref(), Some("Destination"));
    assert_eq!(field.value, Some(json!("Srin")));
    assert_eq!(field.states, ["required"]);
    assert_eq!(field.available_actions, ["Click", "SetValue"]);
    assert_eq!(field.path, ["main", "form"]);
    assert_eq!(field.order, 3);
    assert_eq!(
        field.bounds,
        Some(json!({"x": 10.0, "y": 100.0, "width": 300.0, "height": 40.0}))
    );

    let row = &screen.candidates[2];
    assert_eq!(row.available_actions, ["Click"], "a row takes no text");
    assert!(row.description.is_none() && row.value.is_none());
    assert_eq!(screen.candidates[3].available_actions, ["Click", "Check"]);
    assert!(screen.candidates[4].bounds.is_none());
}

#[test]
fn a_reading_that_failed_or_saw_what_it_cannot_reach_gives_way_to_the_tree() {
    let mut unreachable = reading();
    unreachable["unreachable"] = json!(1);
    assert!(screen(&unreachable).is_none());
    assert!(screen(&json!({"ok": false, "reason": "root not found"})).is_none());
    assert!(screen(&json!(42)).is_none());
    assert!(screen(&json!({"ok": true})).is_none(), "no nodes");

    let bare = screen(&json!({"ok": true, "nodes": []})).unwrap();
    assert_eq!(bare.surface, "window");
    assert!(bare.window.is_none());
}

#[test]
fn a_seen_ref_is_addressed_by_its_mark_and_a_tree_ref_by_itself() {
    assert!(is_seen("seen:12"));
    assert!(!is_seen("e12"));
    assert_eq!(selector("seen:12"), r#"[data-tc-seen="12"]"#);
    assert_eq!(selector("e12"), "@e12");
    assert_eq!(selector("@e12"), "@e12");
}

#[test]
fn the_script_is_called_with_its_root_and_limits() {
    let whole = script(None);
    assert!(whole.starts_with('('), "{}", &whole[..40]);
    assert!(whole.contains("data-tc-seen"));
    assert!(
        whole.ends_with(
            r#"(null, {"controls":800,"labels":3000,"name":120,"text":160,"texts":400})"#
        )
    );
    let under = script(Some("seen:7"));
    assert!(
        under.contains(r#"("[data-tc-seen=\"7\"]", {"#),
        "{}",
        &under[under.len() - 120..]
    );
}

#[test]
fn denoised_summary_parses_and_defaults() {
    let mut read = reading();
    read["denoised"] = json!({"ads": 3, "empty": 2, "hidden": 1});
    assert_eq!(
        denoised(&read),
        Denoised {
            ads: 3,
            empty: 2,
            hidden: 1
        }
    );
    assert!(screen(&read).is_some(), "the summary does not change the screen");

    assert_eq!(
        denoised(&reading()),
        Denoised::default(),
        "a reading from before denoising"
    );
    assert_eq!(
        denoised(&json!({"ok": true, "nodes": [], "denoised": {"ads": 4, "hidden": "x"}})),
        Denoised {
            ads: 4,
            ..Denoised::default()
        },
        "a missing or malformed count is zero"
    );
    assert_eq!(denoised(&json!(42)), Denoised::default());
}

/// Reads `html` by sight in a real browser: `None` unless
/// `TINYCOMPUTER_LIVE_BROWSER=1`, since CI has no browser to launch. The
/// page is written into a blank tab, so nothing is fetched but what the
/// fixture itself asks for.
#[cfg(feature = "agent-browser")]
async fn live_reading(html: &str) -> Option<serde_json::Value> {
    use std::sync::Arc;

    use tinycomputer_bus::browser::SessionOptions;

    use crate::sessions::Browser;

    if std::env::var("TINYCOMPUTER_LIVE_BROWSER").as_deref() != Ok("1") {
        return None;
    }
    let browser = Browser::new(Arc::new(crate::AgentBrowser));
    let info = browser
        .open_session(SessionOptions::default())
        .await
        .expect("a browser launches when live runs are asked for");
    let write = format!(
        "document.open(); document.write({}); document.close(); \
         Promise.all([...document.images].map((image) => image.decode().catch(() => null)))\
         .then(() => true)",
        serde_json::Value::String(html.to_owned())
    );
    browser
        .command(&info.id, json!({"action": "evaluate", "script": write}))
        .await
        .expect("the fixture is written");
    let reply = browser
        .command(&info.id, json!({"action": "evaluate", "script": script(None)}))
        .await;
    browser.close_session(&info.id).await.unwrap();
    Some(reply.expect("sight reads the fixture")["result"].clone())
}

/// The names of the controls and the words of the text a reading returned.
#[cfg(feature = "agent-browser")]
fn shown_names(reading: &serde_json::Value) -> Vec<String> {
    reading["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| {
            node.get("text")
                .or_else(|| node.get("name"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        })
        .collect()
}

#[cfg(feature = "agent-browser")]
#[tokio::test]
async fn live_ad_iframes_and_sponsored_blocks_are_removed() {
    let Some(reading) = live_reading(
        r#"<main>
          <h1>Flights to Srinagar</h1>
          <button>Search</button>
          <iframe src="https://securepubads.g.doubleclick.net/slot" width="900" height="300"></iframe>
          <div class="ad-slot"><a href="https://shop.example/deal">Cheap watches</a></div>
          <div id="div-gpt-ad-1234-0"><button>Ad choices</button></div>
          <div class="box"><p>Advertisement</p>
            <iframe src="https://tpc.googlesyndication.com/x" width="300" height="100"></iframe></div>
          <ul>
            <li><span>Sponsored</span> <a href="https://hotel.example/grand">Grand Hotel</a></li>
            <li><a href="https://inn.example/lake">Lake Inn</a></li>
          </ul>
          <a href="https://ad.doubleclick.net/click?x=1">Buy now</a>
          <img src="https://sb.scorecardresearch.com/p?c1=2" width="1" height="1" alt="">
          <img src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7" alt="">
          <header class="header shadow"><a href="/download">Download app</a></header>
          <div class="badge adults-picker"><button>2 adults</button></div>
          <p class="address">Address: 1 Lake Road</p>
          <div class="css-1ad4k9 sc-hAdSfq"><button>Continue</button></div>
          <div class="AdSlot_wrapper__x1y2"><a href="/deal">Watch deal</a></div>
        </main>"#,
    )
    .await
    else {
        return;
    };
    let names = shown_names(&reading);
    for kept in [
        "Flights to Srinagar",
        "Search",
        "Lake Inn",
        "Download app",
        "2 adults",
        "Address: 1 Lake Road",
        "Continue",
    ] {
        assert!(names.iter().any(|name| name == kept), "{kept} in {names:?}");
    }
    for dropped in [
        "Cheap watches",
        "Ad choices",
        "Advertisement",
        "Sponsored",
        "Grand Hotel",
        "Buy now",
        "Watch deal",
    ] {
        assert!(
            !names.iter().any(|name| name.contains(dropped)),
            "{dropped} in {names:?}"
        );
    }
    assert_eq!(reading["unreachable"], 0, "an ad frame never hides the page");
    assert_eq!(
        reading["denoised"],
        json!({"ads": 9, "empty": 0, "hidden": 0}),
        "{names:?}"
    );
}

#[cfg(feature = "agent-browser")]
#[tokio::test]
async fn live_blank_containers_are_dropped() {
    let Some(reading) = live_reading(
        r#"<main>
          <div style="cursor: pointer; width: 200px; height: 40px"></div>
          <div tabindex="0" style="width: 100px; height: 30px"><span></span></div>
          <div style="cursor: pointer; width: 40px; height: 40px">
            <svg width="20" height="20"><circle r="5" cx="10" cy="10"></circle></svg></div>
          <div style="cursor: pointer"><span>Show more</span></div>
          <button style="width: 50px; height: 30px"></button>
        </main>"#,
    )
    .await
    else {
        return;
    };
    let controls = reading["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| (node["role"].as_str().unwrap(), node["name"].as_str().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        controls,
        [("button", ""), ("button", "Show more"), ("button", "")],
        "a picture's box and a real button stay; blank boxes go"
    );
    assert_eq!(
        reading["denoised"],
        json!({"ads": 0, "empty": 2, "hidden": 0})
    );
}

#[cfg(feature = "agent-browser")]
#[tokio::test]
async fn live_consent_banners_are_kept() {
    let Some(reading) = live_reading(
        r#"<main><button>Search</button></main>
        <div id="cookie-banner" class="consent-banner ad-consent" role="dialog"
             aria-label="Cookie consent">
          <p>We and our advertising partners use cookies</p>
          <button>Accept all</button><button>Reject</button>
        </div>
        <div class="newsletter sponsor-newsletter">
          <label>Email <input type="email"></label><button>Subscribe</button>
        </div>
        <div id="onetrust-banner-sdk" class="ads-consent-bar">
          <p>Personalised ads and cookies</p><button>Allow</button>
        </div>
        <div class="bar"><span>Ad</span> <p>Your privacy choices</p><button>Manage</button></div>"#,
    )
    .await
    else {
        return;
    };
    let names = shown_names(&reading);
    for kept in [
        "Accept all",
        "Reject",
        "Email",
        "Subscribe",
        "Allow",
        "Manage",
        "We and our advertising partners use cookies",
    ] {
        assert!(names.iter().any(|name| name == kept), "{kept} in {names:?}");
    }
}

#[cfg(feature = "agent-browser")]
#[tokio::test]
async fn live_hidden_elements_are_dropped() {
    let Some(reading) = live_reading(
        r#"<main>
          <button>Visible</button>
          <div aria-hidden="true"><button>Clone slide</button><p>Hidden words</p></div>
          <div inert><a href="/x">Inert link</a></div>
          <span style="position: absolute; top: 300px; clip: rect(0 0 0 0)">Screen reader only</span>
          <label><input type="checkbox" style="position: absolute; opacity: 0; width: 1px; height: 1px">
            Keep me signed in</label>
          <label><input type="checkbox"
            style="position: absolute; clip: rect(0 0 0 0); width: 20px; height: 20px"> Send offers</label>
        </main>"#,
    )
    .await
    else {
        return;
    };
    let names = shown_names(&reading);
    assert!(names.iter().any(|name| name == "Visible"), "{names:?}");
    for dropped in [
        "Clone slide",
        "Hidden words",
        "Inert link",
        "Screen reader only",
    ] {
        assert!(!names.contains(&dropped.to_owned()), "{dropped} in {names:?}");
    }
    let checkboxes = reading["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|node| node["role"] == "checkbox")
        .map(|node| node["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        checkboxes,
        ["Keep me signed in", "Send offers"],
        "a label stands in for its hidden box"
    );
    assert_eq!(
        reading["denoised"],
        json!({"ads": 0, "empty": 0, "hidden": 3})
    );

    // A page left marked hidden with nothing in front of it — a modal
    // library that forgot to undo its marking — is still what a person sees.
    let Some(stale) = live_reading(
        r#"<div id="app" aria-hidden="true" style="min-height: 600px">
          <button>Book now</button></div>"#,
    )
    .await
    else {
        return;
    };
    assert_eq!(shown_names(&stale), ["Book now"]);
}
