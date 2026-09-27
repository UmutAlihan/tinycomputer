//! agent-browser, linked in-process, as this crate's engine.
//!
//! Each session gets its own `DaemonState`, built from explicit options so
//! nothing leaks in from the host process's `AGENT_BROWSER_*` environment,
//! and a session id unique to this process so its on-disk bindings never
//! collide with another module's.
//!
//! agent-browser keeps one process-wide piece of state — the active frame a
//! command resolves selectors in — so commands from different sessions are
//! serialized through [`ENGINE`]. Calls on one session are already serialized
//! by the session lock.

use agent_browser::{DaemonState, StateOptions, execute_command};
use serde_json::Value;

use crate::engine::{Engine, Launcher, Reply};

/// Serializes commands across sessions; see the module docs.
static ENGINE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The [`Launcher`] for agent-browser linked in this process.
#[derive(Debug, Clone, Copy, Default)]
pub struct AgentBrowser;

impl Launcher for AgentBrowser {
    fn open(&self, session: &str) -> Box<dyn Engine> {
        Box::new(Linked {
            state: DaemonState::with_options(StateOptions {
                session_id: format!("tinydesktop-{}-{session}", std::process::id()),
                ..StateOptions::default()
            }),
        })
    }
}

struct Linked {
    state: DaemonState,
}

impl Engine for Linked {
    fn execute(&mut self, command: Value) -> Reply<'_> {
        Box::pin(async move {
            let _serialized = ENGINE.lock().await;
            // The dispatcher's future is large (it spans every action), so it
            // is boxed rather than held inline in this one.
            Box::pin(execute_command(&command, &mut self.state)).await
        })
    }
}

#[cfg(test)]
mod test;
