//! Drives the module's engine directly, without a bus in the way.
//!
//! Run it with:
//!
//! ```sh
//! cargo run -p tinycomputer-examples --bin basic
//! ```
//!
//! It calls the members that work on any machine — no permission granted, no
//! application running, no display server — and prints what came back. On a
//! configured macOS or Windows box, uncomment the snapshot at the end to see a
//! real accessibility tree.

use tinycomputer::{Desktop, ListAppsRequest, PermissionsRequest};

fn main() {
    let desktop = Desktop::new();

    let version = desktop.version();
    println!("engine: {}", render(&version));

    // Which permissions this machine has granted. Reporting never prompts;
    // `PermissionsRequest { request: true }` is what puts a dialog on screen.
    let permissions = desktop.permissions(PermissionsRequest::default());
    println!("permissions: {}", render(&permissions));

    // The window server's own list, readable without accessibility access —
    // which makes it the honest first call from an unconfigured machine.
    let apps = desktop.list_apps(ListAppsRequest::default());
    println!("apps: {}", render(&apps));

    // Uncomment on a machine with accessibility access granted:
    //
    // let tree = desktop.snapshot(tinycomputer::SnapshotRequest {
    //     app: Some("Safari".to_owned()),
    //     skeleton: true,
    //     ..Default::default()
    // });
    // println!("tree: {}", render(&tree));
}

/// Renders a reply as the one line that matters: the data, or the error code
/// and what to do about it.
fn render(reply: &tinycomputer::DesktopResponse) -> String {
    match (&reply.data, &reply.error) {
        (Some(data), _) => data.to_string(),
        (None, Some(error)) => match &error.suggestion {
            Some(suggestion) => format!("{} — {} ({suggestion})", error.code, error.message),
            None => format!("{} — {}", error.code, error.message),
        },
        (None, None) => "(empty reply)".to_owned(),
    }
}
