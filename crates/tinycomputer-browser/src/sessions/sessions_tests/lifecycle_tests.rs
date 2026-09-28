//! Tests for opening, capping, and naming sessions, and their scratch space.

use std::sync::Arc;

use tinycomputer_bus::browser::{NavigateRequest, SessionId, SessionOptions};

use super::{open, scratch};
use crate::error::Error;
use crate::fake::{Fake, failure};
use crate::sessions::{Browser, MAX_SESSIONS};

#[tokio::test]
async fn opening_launches_explicitly_then_sets_the_viewport() {
    let fake = Fake::new();
    let (browser, id) = open(&fake, "open").await;
    assert_eq!(fake.actions(), ["launch", "viewport", "url", "title"]);
    let listed = browser.list_sessions().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
    assert_eq!(listed[0].url, "https://flights.test/");
    assert!(listed[0].launched);

    browser.close_session(&id).await.unwrap();
    assert_eq!(fake.last("close")["action"], "close");
    assert!(browser.list_sessions().await.unwrap().is_empty());
    browser
        .close_session(&id)
        .await
        .expect("closing twice succeeds");
}

#[tokio::test]
async fn a_failed_launch_opens_nothing() {
    let fake = Fake::scripted(|command| {
        (command["action"] == "launch").then(|| failure("Auto-launch failed: no chrome"))
    });
    let browser = Browser::with_scratch(Arc::new(fake), scratch("failed"));
    let error = browser
        .open_session(SessionOptions::default())
        .await
        .unwrap_err();
    assert!(
        matches!(error, Error::BrowserUnavailable { .. }),
        "{error:?}"
    );
    assert!(browser.list_sessions().await.unwrap().is_empty());
}

#[tokio::test]
async fn sessions_are_capped_and_unknown_ones_are_named() {
    let fake = Fake::new();
    let browser = Browser::with_scratch(Arc::new(fake), scratch("cap"));
    for _ in 0..MAX_SESSIONS {
        browser
            .open_session(SessionOptions {
                endpoint: Some("ws://127.0.0.1:9222".to_owned()),
                ..SessionOptions::default()
            })
            .await
            .unwrap();
    }
    assert!(matches!(
        browser.open_session(SessionOptions::default()).await,
        Err(Error::LimitExceeded { .. })
    ));
    let missing = SessionId::new("s-missing");
    assert!(matches!(
        browser
            .navigate(&missing, NavigateRequest::new("https://x.test"))
            .await,
        Err(Error::NoSuchSession { .. })
    ));
    assert!(matches!(
        browser.list_downloads(&missing).await,
        Err(Error::NoSuchSession { .. })
    ));
}

#[test]
fn the_default_scratch_space_is_private_to_the_process() {
    let browser = Browser::new(Arc::new(Fake::new()));
    assert!(format!("{browser:?}").contains(&std::process::id().to_string()));
}

/// An engine that yields before every reply, so concurrent launches
/// interleave the way real ones do.
#[derive(Debug)]
struct Slow;

impl crate::engine::Engine for Slow {
    fn execute(&mut self, command: serde_json::Value) -> crate::engine::Reply<'_> {
        Box::pin(async move {
            tokio::task::yield_now().await;
            crate::fake::default_reply(&command)
        })
    }
}

impl crate::engine::Launcher for Slow {
    fn open(&self, _session: &str) -> Box<dyn crate::engine::Engine> {
        Box::new(Slow)
    }
}

#[tokio::test]
async fn concurrent_opens_never_exceed_the_cap() {
    let browser = Arc::new(Browser::with_scratch(Arc::new(Slow), scratch("race")));
    let mut opens = tokio::task::JoinSet::new();
    for _ in 0..MAX_SESSIONS + 4 {
        let browser = browser.clone();
        opens.spawn(async move {
            browser
                .open_session(SessionOptions {
                    endpoint: Some("ws://127.0.0.1:9222".to_owned()),
                    ..SessionOptions::default()
                })
                .await
        });
    }
    let mut refused = 0;
    while let Some(opened) = opens.join_next().await {
        if matches!(opened.unwrap(), Err(Error::LimitExceeded { .. })) {
            refused += 1;
        }
    }
    assert_eq!(refused, 4);
    assert_eq!(browser.list_sessions().await.unwrap().len(), MAX_SESSIONS);
}

#[tokio::test]
async fn a_failed_launch_gives_its_slot_back() {
    let fake =
        Fake::scripted(|command| (command["action"] == "launch").then(|| failure("Chrome exited")));
    let browser = Browser::with_scratch(Arc::new(fake), scratch("slot"));
    // Every launch fails; none may be refused for want of a slot, which is
    // what leaked reservations would cause after MAX_SESSIONS attempts.
    for _ in 0..MAX_SESSIONS + 2 {
        let error = browser
            .open_session(SessionOptions::default())
            .await
            .unwrap_err();
        assert!(!matches!(error, Error::LimitExceeded { .. }), "{error:?}");
    }
}
