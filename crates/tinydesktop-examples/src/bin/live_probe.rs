//! Read-only, filtered accessibility probe through a loaded `TinyBus` module.
//!
//! Run `live_probe <attested-module> <app> <name-fragment>`. Only matching
//! nodes and their immediate parent names are printed.

use std::{io, path::PathBuf, time::Duration};

use serde_json::Value;
use tinybus::{Connection, broker::Broker, module::ModuleHost, transport::memory::MemoryBus};
use tinydesktop_bus::{DesktopResponse, LaunchRequest, SnapshotRequest, names};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let module = PathBuf::from(args.next().ok_or("missing module path")?);
    let app = args
        .next()
        .ok_or("missing app")?
        .to_string_lossy()
        .into_owned();
    let filter = args
        .next()
        .ok_or("missing name fragment")?
        .to_string_lossy()
        .into_owned();
    let bus = MemoryBus::new();
    let broker = Broker::new();
    let task = broker.spawn(bus.clone());
    let host = ModuleHost::new(broker);
    let loaded = host.load_file(module)?;
    if loaded.name != "tinydesktop" {
        return Err(io::Error::other("unexpected module").into());
    }
    let client = Connection::connect(bus.connect().await?).await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if client
                .list_names()
                .await?
                .iter()
                .any(|name| name.as_str() == names::INTERFACE)
            {
                return tinybus::Result::Ok(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;
    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;
    if proxy.attestation().await?.is_none() {
        return Err(io::Error::other("module is not attested").into());
    }
    let launch: DesktopResponse = proxy
        .call(
            names::methods::LAUNCH,
            (LaunchRequest {
                activate: true,
                ..LaunchRequest::new(&app)
            },),
        )
        .await?;
    if !launch.ok {
        return Err(io::Error::other(format!("launch: {:?}", launch.error)).into());
    }
    for _ in 0..30 {
        let reply: DesktopResponse = proxy
            .call(
                names::methods::SNAPSHOT,
                (SnapshotRequest {
                    app: Some(app.clone()),
                    ..SnapshotRequest::default()
                },),
            )
            .await?;
        if reply.ok {
            let tree = require_tree(&reply)?;
            let count = print_matching(tree, &filter, "", 0)?;
            println!("matching_nodes={count}");
            task.abort();
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(io::Error::other("no accessible window after 15 seconds").into())
}

fn require_tree(reply: &DesktopResponse) -> io::Result<&Value> {
    reply
        .data
        .as_ref()
        .and_then(|data| data.get("tree"))
        .filter(|tree| tree.is_object())
        .ok_or_else(|| io::Error::other("successful snapshot has no accessibility tree"))
}

fn print_matching(node: &Value, filter: &str, parent: &str, depth: usize) -> io::Result<usize> {
    if depth > 32 {
        return Err(io::Error::other(
            "accessibility tree exceeds probe depth limit",
        ));
    }
    let name = node.get("name").and_then(Value::as_str).unwrap_or("");
    let description = node
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let role = node.get("role").and_then(Value::as_str).unwrap_or("");
    let mut count = 0;
    let matches = if let Some(wanted_role) = filter.strip_prefix("role:") {
        role.eq_ignore_ascii_case(wanted_role)
    } else {
        name.to_lowercase().contains(&filter.to_lowercase())
            || description.to_lowercase().contains(&filter.to_lowercase())
    };
    if matches && (role != "statictext" || filter.starts_with("role:")) {
        let reference = node.get("ref_id").and_then(Value::as_str).unwrap_or("");
        let states = node.get("states").cloned().unwrap_or(Value::Null);
        println!(
            "role={role:?} name={name:?} description={description:?} parent={parent:?} states={states} ref={reference:?}"
        );
        count += 1;
    }
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        let next_parent = if name.is_empty() { role } else { name };
        for child in children {
            count += print_matching(child, filter, next_parent, depth + 1)?;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::{print_matching, require_tree};
    use serde_json::json;
    use tinydesktop_bus::DesktopResponse;

    #[test]
    fn successful_reply_without_tree_is_an_error() {
        let reply = DesktopResponse::ok("snapshot", json!({"app": "Example"}));
        assert!(require_tree(&reply).is_err());
        let reply = DesktopResponse::ok("snapshot", json!({"tree": null}));
        assert!(require_tree(&reply).is_err());
    }

    #[test]
    fn deep_tree_reports_incomplete_probe() {
        let mut tree = json!({"role": "button", "name": "target"});
        for _ in 0..33 {
            tree = json!({"role": "group", "children": [tree]});
        }
        assert!(print_matching(&tree, "target", "", 0).is_err());
    }

    #[test]
    fn boundary_depth_is_included() {
        let mut tree = json!({"role": "button", "name": "target"});
        for _ in 0..32 {
            tree = json!({"role": "group", "children": [tree]});
        }
        assert!(matches!(print_matching(&tree, "target", "", 0), Ok(1)));
    }
}
