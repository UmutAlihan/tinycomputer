//! Temporary probe; not committed.
use std::sync::Arc;
use tinydesktop_browser::{AgentBrowser, Browser, BrowserSurface, SessionOptions};
use tinydesktop_bus::JevOperation;
use tinydesktop_core::surface::{Depth, Surface};

#[tokio::main]
async fn main() {
    let browser = Arc::new(Browser::new(Arc::new(AgentBrowser)));
    let surface = BrowserSurface::new(
        browser,
        SessionOptions { endpoint: Some("http://127.0.0.1:9222".into()), ..SessionOptions::default() },
        tokio::runtime::Handle::current(),
    );
    tokio::task::spawn_blocking(move || {
        let r = surface.navigate("https://www.google.com/travel/flights?q=Flights%20to%20SXR%20from%20DEL%20on%202026-10-18%20oneway&curr=INR&hl=en&gl=IN");
        println!("nav ok={}", r.ok);
        surface.settle();
        let screen = surface.observe("", None, Depth::Full).unwrap();
        let target = screen.candidates.iter().find(|c| c.name.as_deref().unwrap_or("").starts_with("From ")).cloned().unwrap();
        println!("target {} {} {:?}", target.ref_id, target.role, target.available_actions);
        let reply = surface.execute(JevOperation::Click, Some(target), None);
        println!("ok={} error={:?}", reply.ok, reply.error);
        surface.close();
    }).await.unwrap();
}
