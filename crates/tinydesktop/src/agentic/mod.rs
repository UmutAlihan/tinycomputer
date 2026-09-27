//! Native Jev-backed observation, intent resolution, and goal execution.
//!
//! - `screen` turns a snapshot into the candidates and context Jev sees.
//! - `policy` builds Jev questions and holds the deterministic gates.
//! - `backend` is the engine surface, behind a trait tests fake.
//! - `resolve` is one gated decision; `goal` loops it for `RunGoal`.
//! - `flow` runs high-level intent flows with Jev decision loops.

mod backend;
mod flow;
mod goal;
mod policy;
mod resolve;
mod screen;

#[cfg(test)]
mod test;

use std::{collections::BTreeSet, future::Future, pin::Pin, sync::Arc, time::Duration};

use tinydesktop_bus::{
    DesktopError, DesktopResponse, JevConfig, JevConfiguration, JevDecisionKind, JevMetrics,
    JevProvider, JevTarget, ResolveIntentRequest, RunGoalRequest,
};
use tinyjevclient::{
    Client, ClientConfig, Error as JevError, EvaluationFailure, EvaluationRequest, EvaluationResult,
};

use crate::Desktop;
use backend::AgentBackend;
pub(crate) use flow::{flow_guide, run_flow, validate_flow};
use resolve::{Resolution, resolve};
use screen::Candidate;

/// Configured Jev transport and non-secret policy metadata.
#[derive(Clone)]
pub(crate) struct JevRuntime {
    client: Arc<dyn Evaluator>,
    configuration: JevConfiguration,
}

impl std::fmt::Debug for JevRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JevRuntime")
            .field("client", &"[configured]")
            .field("configuration", &self.configuration)
            .finish()
    }
}

impl JevRuntime {
    pub(crate) fn configure(request: &JevConfig) -> Result<Self, Box<DesktopError>> {
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
        })
    }
}

trait Evaluator: Send + Sync {
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
        Box::pin(Client::evaluate(self, request))
    }
}

fn trusted_endpoint(provider: JevProvider, endpoint: &str) -> bool {
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

pub(crate) async fn resolve_intent(
    desktop: Desktop,
    runtime: JevRuntime,
    request: ResolveIntentRequest,
) -> DesktopResponse {
    resolve_intent_with(desktop, runtime, request).await
}

async fn resolve_intent_with<B: AgentBackend>(
    backend: B,
    runtime: JevRuntime,
    request: ResolveIntentRequest,
) -> DesktopResponse {
    let banned = BTreeSet::new();
    let result = resolve(
        &backend,
        &runtime,
        &request.app,
        request.root.as_deref(),
        Resolution {
            intent: &request.intent,
            text: request.text.as_deref(),
            include_values: request.include_values,
            execute: request.execute,
            history: &[],
            allow_rerank: true,
            banned: &banned,
        },
    )
    .await;
    match result {
        Ok(outcome) => outcome
            .action_failure
            .unwrap_or_else(|| response("resolve-intent", &outcome.decision)),
        Err(error) => *error,
    }
}

pub(crate) async fn run_goal(
    desktop: Desktop,
    runtime: JevRuntime,
    request: RunGoalRequest,
) -> DesktopResponse {
    goal::run_goal_with(desktop, runtime, request).await
}

fn target_payload(candidate: &Candidate) -> JevTarget {
    JevTarget {
        ref_id: candidate.ref_id.clone(),
        role: candidate.role.clone(),
        name: candidate
            .name
            .clone()
            .or_else(|| candidate.description.clone()),
    }
}

fn reason(decision: JevDecisionKind, confidence: f64, destructive: f64) -> String {
    match decision {
        JevDecisionKind::Act => "the target cleared the safe-action threshold".to_owned(),
        JevDecisionKind::ConfirmationRequired => {
            format!("the action is hard to undo ({destructive:.2})")
        }
        JevDecisionKind::Abstain => format!("target confidence {confidence:.2} is too low"),
        JevDecisionKind::NeedsText => {
            "the selected operation needs caller-supplied text".to_owned()
        }
        JevDecisionKind::Done => "the goal is visibly satisfied".to_owned(),
        JevDecisionKind::Blocked => "no offered operation can advance the goal".to_owned(),
    }
}

fn merge_metrics(metrics: &mut JevMetrics, evaluation: &EvaluationResult) {
    metrics.calls = metrics.calls.saturating_add(1);
    metrics.attempts = metrics.attempts.saturating_add(evaluation.attempts);
    metrics.latency_ms = metrics.latency_ms.saturating_add(
        evaluation
            .latency
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
    );
    metrics.input_tokens = metrics
        .input_tokens
        .saturating_add(evaluation.response.usage.input_tokens.unwrap_or_default());
    metrics.output_tokens = metrics
        .output_tokens
        .saturating_add(evaluation.response.usage.output_tokens.unwrap_or_default());
    metrics.model = Some(evaluation.response.model.clone());
}

fn response<T: serde::Serialize>(command: &str, value: &T) -> DesktopResponse {
    match serde_json::to_value(value) {
        Ok(data) => DesktopResponse::ok(command, data),
        Err(error) => internal_error(&format!("cannot encode Jev result: {error}")),
    }
}

fn config_error(error: &JevError) -> Box<DesktopError> {
    Box::new(DesktopError::new("JEV_INVALID_CONFIG", error.to_string()))
}

fn provider_error(error: &tinyjevclient::EvaluationFailure) -> Box<DesktopResponse> {
    let code = match &error.error {
        JevError::Authentication => "JEV_AUTHENTICATION",
        JevError::RateLimited => "JEV_RATE_LIMITED",
        JevError::Timeout => "JEV_TIMEOUT",
        JevError::InvalidResponse { .. } | JevError::Decode { .. } => "JEV_INVALID_RESPONSE",
        _ => "JEV_PROVIDER_FAILED",
    };
    Box::new(DesktopResponse::err(
        "jev-evaluate",
        DesktopError::new(code, error.to_string()),
    ))
}

fn invalid_response(message: &str) -> Box<DesktopResponse> {
    Box::new(DesktopResponse::err(
        "jev-evaluate",
        DesktopError::new("JEV_INVALID_RESPONSE", message),
    ))
}

fn internal_error(message: &str) -> DesktopResponse {
    DesktopResponse::err("jev-desktop", DesktopError::new("INTERNAL", message))
}
