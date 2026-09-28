//! Live tests that read fixture pages by sight in a real browser, gated on
//! `TINYCOMPUTER_LIVE_BROWSER=1`.

#[cfg(feature = "agent-browser")]
use serde_json::json;

#[cfg(feature = "agent-browser")]
use crate::surface::sight::script;

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
        .command(
            &info.id,
            json!({"action": "evaluate", "script": script(None)}),
        )
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
          <div class="gb_2d gb_Ad"><button>Main menu</button></div>
          <div class="gb_ad"><button>Apps</button></div>
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
        "Main menu",
        "Apps",
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
    assert_eq!(
        reading["unreachable"], 0,
        "an ad frame never hides the page"
    );
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
        .map(|node| {
            (
                node["role"].as_str().unwrap(),
                node["name"].as_str().unwrap(),
            )
        })
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
          <div aria-hidden="true" style="position: absolute; left: 1400px; top: 100px">
            <button>Clone slide</button><p>Hidden words</p></div>
          <div inert><a href="/x">Inert link</a></div>
          <span style="position: absolute; top: 300px; clip: rect(0 0 0 0)">Screen reader only</span>
          <label><input type="checkbox" style="position: absolute; opacity: 0; width: 1px; height: 1px">
            Keep me signed in</label>
          <label><input type="checkbox"
            style="position: absolute; clip: rect(0 0 0 0); width: 20px; height: 20px"> Send offers</label>
          <p><span aria-hidden="true">Sort by:</span></p>
          <div aria-hidden="true" style="margin-top: 1200px"><button>Explore destinations</button></div>
        </main>"#,
    )
    .await
    else {
        return;
    };
    let names = shown_names(&reading);
    for kept in ["Visible", "Sort by:", "Explore destinations"] {
        assert!(names.iter().any(|name| name == kept), "{kept} in {names:?}");
    }
    for dropped in [
        "Clone slide",
        "Hidden words",
        "Inert link",
        "Screen reader only",
    ] {
        assert!(
            !names.contains(&dropped.to_owned()),
            "{dropped} in {names:?}"
        );
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

    // The page a dialog hides is left out; the dialog is what is read.
    let Some(behind) = live_reading(
        r#"<main aria-hidden="true"><button>Behind the dialog</button><p>Page words</p></main>
        <div role="dialog" aria-modal="true" style="position: fixed; inset: 0; background: white">
          <button>Close</button></div>"#,
    )
    .await
    else {
        return;
    };
    assert_eq!(shown_names(&behind), ["Close"]);
    assert_eq!(behind["denoised"]["hidden"], 1);

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
