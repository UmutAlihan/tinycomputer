//! The Jev runtime: its configured client, the journal it writes, and the
//! one door every Jev evaluation goes through.

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

use tinycomputer_bus::{DesktopError, JevConfig, JevConfiguration, JevProvider};
use tinyinference_decisions::{
    Client, ClientConfig, Error as JevError, EvaluationFailure, EvaluationRequest, EvaluationResult,
};

use super::journal::Journal;
use super::pending::PendingRun;
use super::sage;

/// Configured Jev transport and non-secret policy metadata.
#[derive(Clone)]
pub struct JevRuntime {
    pub(super) client: Arc<dyn Evaluator>,
    pub(super) configuration: JevConfiguration,
    pub(super) pending: Arc<Mutex<HashMap<String, PendingRun>>>,
    journal: Journal,
}

impl std::fmt::Debug for JevRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JevRuntime")
            .field("client", &"[configured]")
            .field("configuration", &self.configuration)
            .field("pending", &"[redacted]")
            .field("journal", &self.journal)
            .finish()
    }
}

impl JevRuntime {
    /// Build a runtime from the module's private `jev` configuration.
    ///
    /// # Errors
    ///
    /// Returns a `JEV_INVALID_CONFIG` [`DesktopError`] when the provider,
    /// endpoint, or credentials in `request` cannot form a trusted client.
    pub fn configure(request: &JevConfig) -> Result<Self, Box<DesktopError>> {
        let mut config = match request.provider {
            JevProvider::TypeSafe => ClientConfig::new(request.api_key()),
            JevProvider::OpenRouter => ClientConfig::openrouter(request.api_key()),
            JevProvider::TinyHumansOpenRouter => {
                ClientConfig::tinyhumans_openrouter(request.api_key())
            }
        };
        if let Some(endpoint) = &request.endpoint_url {
            if !trusted_endpoint(request.provider, endpoint) {
                return Err(Box::new(DesktopError::new(
                    "JEV_INVALID_CONFIG",
                    "endpoint is not an approved Jev provider route",
                )));
            }
            config = config.with_endpoint_url(endpoint);
        }
        if let Some(timeout_ms) = request.timeout_ms {
            config.timeout = Duration::from_millis(timeout_ms);
        }
        if let Some(max_retries) = request.max_retries {
            config.retry.max_retries = max_retries;
        }
        if request.provider == JevProvider::TinyHumansOpenRouter
            && let Some(sdk_name) = request.sdk_name.as_deref()
        {
            config = config.with_sdk_name(sdk_name);
        }
        let client = Client::new(config).map_err(|error| config_error(&error))?;
        Ok(Self {
            client: Arc::new(client),
            configuration: JevConfiguration {
                provider: request.provider,
                model: request
                    .model
                    .clone()
                    .unwrap_or_else(|| "jev-latest".to_owned()),
                endpoint_url: request.endpoint_url.clone(),
            },
            pending: Arc::new(Mutex::new(HashMap::new())),
            journal: Journal::from_env(),
        })
    }

    /// A runtime whose decisions Levanto Sage makes in place of Jev, with
    /// `api_key`; `fast` scores each choice in one pass rather than one per
    /// option.
    ///
    /// For measuring Sage behind the same loops (`agentic/sage/`): it is not
    /// reachable over the bus, and [`JevConfiguration`] names it by its model,
    /// `levanto-sage`.
    ///
    /// # Errors
    ///
    /// Returns a `JEV_INVALID_CONFIG` [`DesktopError`] when `api_key` is
    /// empty.
    pub fn sage(api_key: &str, fast: bool) -> Result<Self, Box<DesktopError>> {
        let client = tinyinference_decisions::sage::SageClient::new(api_key)
            .map_err(|error| config_error(&error))?;
        Ok(Self {
            client: Arc::new(sage::SageEvaluator::new(client, fast)),
            configuration: JevConfiguration {
                provider: JevProvider::TypeSafe,
                model: "levanto-sage".to_owned(),
                endpoint_url: None,
            },
            pending: Arc::new(Mutex::new(HashMap::new())),
            journal: Journal::from_env(),
        })
    }

    /// This runtime with the debug journal written under `dir`, whatever
    /// [`JOURNAL_ENV`] says.
    ///
    /// Every Jev exchange of every run, with its latency, and the time each
    /// flow spends observing, acting, and on each step, is appended to
    /// `<dir>/<run id>/journal.jsonl`. See `docs/jev-journal.md`.
    #[must_use]
    pub fn with_journal(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.journal = Journal::at(dir);
        self
    }

    /// This runtime writing its journal to the run named `run_id`, so the
    /// several runs of one task — a flow and each continuation — share one
    /// journal file. Does nothing when the journal is off.
    #[must_use]
    pub fn journaled_as(&self, run_id: &str) -> Self {
        Self {
            journal: self.journal.named(run_id),
            ..self.clone()
        }
    }

    /// The directory this runtime's current run journal is written to, if
    /// the journal is on and a run has begun.
    #[must_use]
    pub fn journal_dir(&self) -> Option<std::path::PathBuf> {
        self.journal.run_dir()
    }

    /// This runtime writing to `journal`.
    pub(super) fn within(self, journal: Journal) -> Self {
        Self { journal, ..self }
    }

    /// This runtime with a run of `kind` begun in its journal.
    pub(super) fn begin_run(&self, kind: &str, label: &str) -> Self {
        Self {
            journal: self.journal.begin(kind, label, &self.configuration.model),
            ..self.clone()
        }
    }

    /// Asks Jev one request, journaling the exchange against `step`.
    pub(super) async fn evaluate(
        &self,
        step: Option<&str>,
        request: &EvaluationRequest,
    ) -> std::result::Result<EvaluationResult, EvaluationFailure> {
        let outcome = self.client.evaluate(request).await;
        self.journal.exchange(step, request, outcome.as_ref());
        outcome
    }
}

pub(super) trait Evaluator: Send + Sync {
    fn evaluate<'a>(
        &'a self,
        request: &'a EvaluationRequest,
    ) -> Pin<
        Box<
            dyn Future<Output = std::result::Result<EvaluationResult, EvaluationFailure>>
                + Send
                + 'a,
        >,
    >;
}

impl Evaluator for Client {
    fn evaluate<'a>(
        &'a self,
        request: &'a EvaluationRequest,
    ) -> Pin<
        Box<
            dyn Future<Output = std::result::Result<EvaluationResult, EvaluationFailure>>
                + Send
                + 'a,
        >,
    > {
        // The decisions client reports failures as its crate error; the loops
        // want the failure record with its attempts and latency.
        Box::pin(async move {
            Client::evaluate(self, request)
                .await
                .map_err(|error| match error {
                    JevError::EvaluationFailure(failure) => failure,
                    other => EvaluationFailure {
                        error: Box::new(other),
                        attempts: 0,
                        latency: Duration::ZERO,
                    },
                })
        })
    }
}

pub(super) fn trusted_endpoint(provider: JevProvider, endpoint: &str) -> bool {
    let approved = match provider {
        JevProvider::TypeSafe => "https://api.typesafe.ai/v1/systemone",
        JevProvider::OpenRouter => "https://openrouter.ai/api/alpha/decisions",
        JevProvider::TinyHumansOpenRouter => {
            "https://api.tinyhumans.ai/agent-integrations/openrouter/systemone"
        }
    };
    if endpoint == approved {
        return true;
    }
    #[cfg(test)]
    return endpoint.starts_with("http://127.0.0.1:");
    #[cfg(not(test))]
    false
}

pub(super) fn config_error(error: &JevError) -> Box<DesktopError> {
    Box::new(DesktopError::new("JEV_INVALID_CONFIG", error.to_string()))
}
