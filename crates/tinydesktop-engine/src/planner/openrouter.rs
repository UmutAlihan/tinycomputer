//! The planner's model on `OpenRouter`, through `tinyinference-llm`.
//!
//! Only this file links a text-generating model, and only with the `planner`
//! feature. The key arrives in the module's private configuration and never
//! leaves this adapter.

use std::sync::Arc;

use serde::Deserialize;
use tinyinference_llm::model::ResponseFormat;
use tinyinference_llm::providers::openai::OpenAiModel;
use tinyinference_llm::{ChatModel, Message, ModelRequest, ProviderKind, ProviderSpec};

use super::{Completion, LanguageModel, Planner, Role, Turn};

/// The model used when the configuration names none.
pub const PLANNER_MODEL: &str = "anthropic/claude-sonnet-5";

/// The module's private `planner` configuration.
#[derive(Clone, Deserialize)]
pub struct PlannerConfig {
    /// The `OpenRouter` key.
    pub api_key: String,
    /// The `OpenRouter` model id; [`PLANNER_MODEL`] when absent.
    #[serde(default)]
    pub model: Option<String>,
}

impl std::fmt::Debug for PlannerConfig {
    /// Never prints the key.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PlannerConfig")
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

/// A [`Planner`] on `OpenRouter` configured by `config`.
///
/// # Errors
///
/// Why the model could not be built, such as an empty key.
pub fn open_router(config: &PlannerConfig) -> Result<Planner, String> {
    if config.api_key.trim().is_empty() {
        return Err("the planner configuration needs an api_key".to_owned());
    }
    let model = config
        .model
        .clone()
        .filter(|model| !model.trim().is_empty())
        .unwrap_or_else(|| PLANNER_MODEL.to_owned());
    let spec = ProviderSpec::for_kind(ProviderKind::OpenRouter).with_model(model);
    let model = OpenAiModel::from_spec(spec, config.api_key.clone())
        .map_err(|error| error.to_string())?
        .with_json_object_format(true);
    Ok(Planner::new(Arc::new(OpenRouter(Arc::new(model)))))
}

struct OpenRouter(Arc<OpenAiModel>);

impl LanguageModel for OpenRouter {
    fn complete(&self, turns: &[Turn]) -> Completion {
        let model = self.0.clone();
        let messages = turns
            .iter()
            .map(|turn| match turn.role {
                Role::System => Message::system(turn.text.clone()),
                Role::User => Message::user(turn.text.clone()),
                Role::Assistant => Message::assistant(turn.text.clone()),
            })
            .collect();
        Box::pin(async move {
            let request = ModelRequest {
                messages,
                response_format: Some(ResponseFormat::JsonObject),
                temperature: Some(0.2),
                max_tokens: Some(4_000),
                ..ModelRequest::default()
            };
            let response = model
                .invoke(&(), request)
                .await
                .map_err(|error| error.to_string())?;
            Ok(Message::Assistant(response.message).text())
        })
    }
}
