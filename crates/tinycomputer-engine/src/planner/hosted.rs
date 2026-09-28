//! The planner's, the rescuer's, and the shaper's models on a hosted
//! OpenAI-compatible route — `OpenRouter` or Tiny Humans' gateway — through
//! `tinyinference-llm`.
//!
//! Only this file links a text-generating model, and only with the `planner`
//! feature. The keys arrive in the module's private configuration and never
//! leave this adapter.

use std::sync::Arc;

use tinycomputer_bus::agent::LanguageModelProvider;
use tinyinference_llm::model::{ReasoningConfig, ReasoningEffort, ResponseFormat};
use tinyinference_llm::providers::openai::OpenAiModel;
use tinyinference_llm::{ChatModel, Message, ModelRequest, ProviderKind, ProviderSpec};

use super::config::{ModelRoute, OUTPUT_MODEL, PLANNER_MODEL, PlannerConfig, RESCUE_MODEL};
use super::{Completion, LanguageModel, Planner, Role, Turn};
use crate::rescue::Rescuer;
use crate::shape::Shaper;

/// A [`Planner`] configured by `config`, on its route: `OpenRouter` unless
/// `provider` says `tiny_humans`.
///
/// # Errors
///
/// Why the model could not be built, such as an empty key or an endpoint
/// that is not the provider's own.
pub fn open_router(config: &PlannerConfig) -> Result<Planner, String> {
    let model = model_name(config.model.as_ref(), PLANNER_MODEL);
    let chat = chat_model(&config.route, &model)?;
    Ok(Planner::new(Arc::new(Hosted {
        model: chat,
        temperature: Some(0.2),
        reasoning: None,
        max_tokens: 4_000,
    }))
    .with_configuration(config.route.describe(&model)))
}

/// A [`Rescuer`] configured by `config`: the `rescue_model` (or
/// [`RESCUE_MODEL`]) on `rescue_route` (or the planner's route), reasoning
/// briefly before it answers.
///
/// # Errors
///
/// Why the model could not be built, such as an empty key or an endpoint
/// that is not the provider's own.
pub fn open_router_rescuer(config: &PlannerConfig) -> Result<Rescuer, String> {
    let model = model_name(config.rescue_model.as_ref(), RESCUE_MODEL);
    let route = config.rescuer_route();
    let chat = chat_model(route, &model)?;
    Ok(Rescuer::new(Arc::new(Hosted {
        model: chat,
        // Reasoning models take no sampling temperature.
        temperature: None,
        reasoning: Some(ReasoningEffort::Low),
        max_tokens: 8_000,
    }))
    .with_configuration(route.describe(&model)))
}

/// A [`Shaper`] configured by `config`: the `output_model` (or
/// [`OUTPUT_MODEL`]) on the planner's route, reasoning briefly before it
/// answers.
///
/// # Errors
///
/// Why the model could not be built, such as an empty key or an endpoint
/// that is not the provider's own.
pub fn open_router_shaper(config: &PlannerConfig) -> Result<Shaper, String> {
    let model = model_name(config.output_model.as_ref(), OUTPUT_MODEL);
    let chat = chat_model(&config.route, &model)?;
    Ok(Shaper::new(Arc::new(Hosted {
        model: chat,
        // Reasoning models take no sampling temperature.
        temperature: None,
        reasoning: Some(ReasoningEffort::Low),
        // Room for a result built from many records.
        max_tokens: 16_000,
    }))
    .with_configuration(config.route.describe(&model)))
}

/// `configured`, unless absent or blank, else `default`.
fn model_name(configured: Option<&String>, default: &str) -> String {
    configured
        .filter(|model| !model.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| default.to_owned())
}

/// The OpenAI-compatible client for `model` on `route`.
pub(super) fn chat_model(route: &ModelRoute, model: &str) -> Result<Arc<OpenAiModel>, String> {
    let resolved = route.resolve()?;
    let kind = match resolved.provider {
        LanguageModelProvider::OpenRouter => ProviderKind::OpenRouter,
        LanguageModelProvider::TinyHumans => ProviderKind::TinyHumans,
    };
    let spec = ProviderSpec::for_kind(kind)
        .with_model(model)
        .with_base_url(resolved.base_url);
    let mut chat = OpenAiModel::from_spec(spec, route.api_key.clone())
        .map_err(|error| error.to_string())?
        .with_json_object_format(true);
    if let Some(sdk_name) = resolved.sdk_name {
        chat = chat.with_header("x-sdk-name", sdk_name);
    }
    Ok(Arc::new(chat))
}

struct Hosted {
    model: Arc<OpenAiModel>,
    temperature: Option<f64>,
    reasoning: Option<ReasoningEffort>,
    max_tokens: u32,
}

impl LanguageModel for Hosted {
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
