//! Loads the built module through the real `TinyBus` loader and calls it.
//!
//! This is the same path a production host takes: the dylib is attested
//! against `modules.toml`, loaded into a broker, given its private Jev
//! configuration by reinitialization, and called over a connection. Nothing
//! here reaches into the module's Rust API.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tinybus::{Connection, broker::Broker, module::ModuleHost, transport::memory::MemoryBus};
use tinydesktop_bus::{
    DesktopResponse, FlowRunResult, FlowValidation, JevConfig, JevProvider, JevRunResult,
    RunFlowRequest, RunGoalRequest, ValidateFlowRequest, names,
};

/// Boxed error for the lab's command-line plumbing.
pub type LabError = Box<dyn std::error::Error + Send + Sync>;

/// A loaded, configured tinydesktop module and a proxy to it.
pub struct Host {
    proxy: tinybus::Proxy,
    broker: tokio::task::JoinHandle<()>,
    _client: Connection,
}

impl std::fmt::Debug for Host {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Host").finish_non_exhaustive()
    }
}

/// How the lab configures the module it loads.
#[derive(Debug, Clone, Default)]
pub struct HostOptions {
    /// Use physical input so the cursor visibly moves.
    pub headed: bool,
    /// Jev model or alias; `jev-latest` when absent.
    pub jev_model: Option<String>,
}

impl Host {
    /// Loads the module at `module`, which must be listed in the
    /// `modules.toml` beside it, and configures Jev from `OPENROUTER_API_KEY`.
    ///
    /// # Errors
    ///
    /// Fails when the module is not attested, cannot load, or the key is not
    /// exported.
    pub async fn load(module: &Path, options: &HostOptions) -> Result<Self, LabError> {
        verify_allowlisted(module)?;
        let key = std::env::var("OPENROUTER_API_KEY")
            .map_err(|_| io::Error::other("OPENROUTER_API_KEY is not exported"))?;
        let bus = MemoryBus::new();
        let broker = Broker::new();
        let broker_task = broker.spawn(bus.clone());
        let module_host = ModuleHost::new(broker);
        let info = module_host.load_file(module)?;
        if info.name != "tinydesktop" {
            return Err(io::Error::other(format!("loaded unexpected module `{}`", info.name)).into());
        }
        let client = Connection::connect(bus.connect().await?).await?;
        wait_for_module(&client).await?;
        let mut jev = JevConfig::new(key);
        jev.provider = JevProvider::OpenRouter;
        jev.endpoint_url = Some("https://openrouter.ai/api/alpha/decisions".to_owned());
        jev.model = Some(
            options
                .jev_model
                .clone()
                .unwrap_or_else(|| "jev-latest".to_owned()),
        );
        client
            .reinitialize_module(
                "tinydesktop",
                json!({"jev": jev, "headed": options.headed, "session_id": "tinydesktop-lab"}),
            )
            .await?;
        let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;
        if proxy.attestation().await?.is_none() {
            return Err(io::Error::other(
                "tinydesktop is not attested; run scripts/lab so modules.toml is generated",
            )
            .into());
        }
        Ok(Self {
            proxy,
            broker: broker_task,
            _client: client,
        })
    }

    /// Calls `member` with one positional argument and returns the envelope.
    ///
    /// # Errors
    ///
    /// Fails only on a transport error; a failed command is an `ok: false`
    /// envelope.
    pub async fn call(&self, member: &str, argument: Value) -> Result<DesktopResponse, LabError> {
        let arguments = if argument.is_null() {
            json!([])
        } else {
            json!([argument])
        };
        Ok(self.proxy.call(member, arguments).await?)
    }

    /// Runs a flow.
    ///
    /// # Errors
    ///
    /// Fails on a transport error or an error envelope.
    pub async fn run_flow(&self, request: &RunFlowRequest) -> Result<FlowRunResult, LabError> {
        let reply = self
            .call(names::methods::RUN_FLOW, serde_json::to_value(request)?)
            .await?;
        data(reply)
    }

    /// Runs a goal with the single-goal Jev loop, for comparison.
    ///
    /// # Errors
    ///
    /// Fails on a transport error or an error envelope.
    pub async fn run_goal(&self, request: &RunGoalRequest) -> Result<JevRunResult, LabError> {
        let reply = self
            .call(names::methods::RUN_GOAL, serde_json::to_value(request)?)
            .await?;
        data(reply)
    }

    /// Validates a candidate flow.
    ///
    /// # Errors
    ///
    /// Fails on a transport error.
    pub async fn validate(&self, flow: &Value) -> Result<FlowValidation, LabError> {
        let reply = self
            .call(
                names::methods::VALIDATE_FLOW,
                serde_json::to_value(ValidateFlowRequest { flow: flow.clone() })?,
            )
            .await?;
        data(reply)
    }

    /// The flow authoring guide.
    ///
    /// # Errors
    ///
    /// Fails on a transport error.
    pub async fn guide(&self) -> Result<String, LabError> {
        let reply = self.call(names::methods::FLOW_GUIDE, Value::Null).await?;
        let value: Value = data(reply)?;
        Ok(value
            .get("guide")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned())
    }

    /// Stops the in-memory broker.
    pub fn shutdown(self) {
        self.broker.abort();
    }
}

/// The payload of a successful envelope, or its error as a lab error.
///
/// # Errors
///
/// Fails when the envelope is an error or its data does not decode as `T`.
pub fn data<T: DeserializeOwned>(reply: DesktopResponse) -> Result<T, LabError> {
    if let Some(error) = reply.error {
        return Err(io::Error::other(format!(
            "{} failed: {}: {}",
            reply.command, error.code, error.message
        ))
        .into());
    }
    Ok(serde_json::from_value(reply.data.unwrap_or(Value::Null))?)
}

async fn wait_for_module(client: &Connection) -> Result<(), LabError> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if client
                .list_names()
                .await?
                .iter()
                .any(|name| name.as_str() == names::INTERFACE)
            {
                return Ok::<(), tinybus::Error>(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| io::Error::other("timed out waiting for tinydesktop"))??;
    Ok(())
}

fn verify_allowlisted(module: &Path) -> Result<(), LabError> {
    let file_name = module
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::other("module path has no UTF-8 file name"))?;
    let manifest_path = module
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("modules.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .map_err(|_| io::Error::other("modules.toml is required beside the module"))?;
    let prefix = format!("\"{file_name}\" = \"");
    let expected = manifest
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| io::Error::other("module is absent from modules.toml"))?;
    let observed = tinybus::module::sha256_file(module)?;
    if observed != expected {
        return Err(io::Error::other(
            "module checksum does not match modules.toml; rebuild with scripts/lab",
        )
        .into());
    }
    Ok(())
}

/// The module path from `TINYDESKTOP_MODULE`, or the lab's default build.
#[must_use]
pub fn module_path() -> PathBuf {
    std::env::var_os("TINYDESKTOP_MODULE").map_or_else(
        || PathBuf::from(format!("target/lab/{}", library_name())),
        PathBuf::from,
    )
}

fn library_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "libtinydesktop.dylib"
    } else if cfg!(target_os = "windows") {
        "tinydesktop.dll"
    } else {
        "libtinydesktop.so"
    }
}
