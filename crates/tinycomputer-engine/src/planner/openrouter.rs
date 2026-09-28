//! The planner's and the rescuer's models on `OpenRouter`, through
//! `tinyinference-llm`.
//!
//! Only this file links a text-generating model, and only with the `planner`
//! feature. The key arrives in the module's private configuration and never
//! leaves this adapter.

use std::sync::Arc;

use serde::Deserialize;
use tinyinference_llm::model::{ReasoningConfig, ReasoningEffort, ResponseFormat};
use tinyinference_llm::providers::openai::OpenAiModel;
use tinyinference_llm::{ChatModel, Message, ModelRequest, ProviderKind, ProviderSpec};

use super::{Completion, LanguageModel, Planner, Role, Turn};
use crate::rescue::Rescuer;

/// The model used when the configuration names none.
pub const PLANNER_MODEL: &str = "anthropic/claude-sonnet-5";

/// The reasoning model a failed step is rescued with when the configuration
/// names none.
pub const RESCUE_MODEL: &str = "openai/gpt-6-luna";

/// The module's private `planner` configuration.
#[derive(Clone, Deserialize)]
pub struct PlannerConfig {
    /// The `OpenRouter` key.
    pub api_key: String,
    /// The `OpenRouter` model id; [`PLANNER_MODEL`] when absent.
    #[serde(default)]
    pub model: Option<String>,
    /// The `OpenRouter` model id failed steps are rescued with;
    /// [`RESCUE_MODEL`] when absent.
    #[serde(default)]
    pub rescue_model: Option<String>,
}

impl std::fmt::Debug for PlannerConfig {
    /// Never prints the key.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PlannerConfig")
            .field("model", &self.model)
            .field("rescue_model", &self.rescue_model)
            .finish_non_exhaustive()
    }
}

/// A [`Planner`] on `OpenRouter` configured by `config`.
///
/// # Errors
///
/// Why the model could not be built, such as an empty key.
pub fn open_router(config: &PlannerConfig) -> Result<Planner, String> {
    let model = chat_model(config, config.model.as_ref(), PLANNER_MODEL)?;
    Ok(Planner::new(Arc::new(OpenRouter {
        model,
        temperature: Some(0.2),
        reasoning: None,
        max_tokens: 4_000,
    })))
}

/// A [`Rescuer`] on `OpenRouter` configured by `config`: the same key, the
/// `rescue_model` (or [`RESCUE_MODEL`]), reasoning briefly before it answers.
///
/// # Errors
///
/// Why the model could not be built, such as an empty key.
pub fn open_router_rescuer(config: &PlannerConfig) -> Result<Rescuer, String> {
    let model = chat_model(config, config.rescue_model.as_ref(), RESCUE_MODEL)?;
    Ok(Rescuer::new(Arc::new(OpenRouter {
        model,
        // Reasoning models take no sampling temperature.
        temperature: None,
        reasoning: Some(ReasoningEffort::Low),
        max_tokens: 8_000,
    })))
}

fn chat_model(
    config: &PlannerConfig,
    model: Option<&String>,
    default: &str,
) -> Result<Arc<OpenAiModel>, String> {
    if config.api_key.trim().is_empty() {
        return Err("the planner configuration needs an api_key".to_owned());
    }
    let model = model
        .filter(|model| !model.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| default.to_owned());
    let spec = ProviderSpec::for_kind(ProviderKind::OpenRouter).with_model(model);
    let model = OpenAiModel::from_spec(spec, config.api_key.clone())
        .map_err(|error| error.to_string())?
        .with_json_object_format(true);
    Ok(Arc::new(model))
}

struct OpenRouter {
    model: Arc<OpenAiModel>,
    temperature: Option<f64>,
    reasoning: Option<ReasoningEffort>,
    max_tokens: u32,
}

impl LanguageModel for OpenRouter {
    fn complete(&self, turns: &[Turn]) -> Completion {
        let model = self.model.clone();
        let messages = turns
            .iter()
            .map(|turn| match turn.role {
                Role::System => Message::system(turn.text.clone()),
                Role::User => Message::user(turn.text.clone()),
                Role::Assistant => Message::assistant(turn.text.clone()),
            })
            .collect();
        let request = ModelRequest {
            messages,
            response_format: Some(ResponseFormat::JsonObject),
            temperature: self.temperature,
            max_tokens: Some(self.max_tokens),
            reasoning: self.reasoning.map(ReasoningConfig::effort),
            ..ModelRequest::default()
        };
        Box::pin(async move {
            let response = model
                .invoke(&(), request)
                .await
                .map_err(|error| error.to_string())?;
            Ok(Message::Assistant(response.message).text())
        })
    }
}
