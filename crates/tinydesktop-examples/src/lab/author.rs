//! The optional LLM layer: a model that writes flows, never clicks.
//!
//! The author is given the task brief and the module's own `FlowGuide`, and
//! nothing about the application's interface. It answers with a flow; the lab
//! validates it through `ValidateFlow` (repairing up to twice) and runs it with
//! Jev. After each run the author sees a summary — including anything `read`
//! steps captured — and either declares the task done or writes a flow for
//! what remains. That round trip is how a reply gets written about a message
//! the author has never seen: the flow reads it, the author writes the text.
//!
//! This layer is lab-only. The shipped module stays Jev-only and links none
//! of it.

use serde_json::Value;
use tinyinference_llm::{
    ChatModel, ContentBlock, Message, ModelRequest, ProviderKind, ProviderSpec,
    message::{ImageRef, UserMessage},
    model::ResponseFormat,
    providers::openai::OpenAiModel,
};

use super::host::{Host, LabError};

/// The `OpenRouter` model used when `TINYDESKTOP_LAB_MODEL` is not set.
pub const DEFAULT_MODEL: &str = "anthropic/claude-sonnet-5";
/// Validation repairs per authored flow.
const REPAIRS: usize = 2;

const PROTOCOL: &str = "You write desktop flows for a desktop-automation module. \
You cannot see the screen and you do not know the application's interface; you \
only describe what should happen, in plain steps, following the guide below. \
Reply with exactly one JSON object and nothing else: either a flow \
({\"app\": ..., \"steps\": [...]}) or, when the task is complete, {\"done\": true}. \
Put every word that should appear on screen into the flow yourself. \
Always guard sending, deleting, or submitting with a stop_before step.";

/// A conversation with the authoring model.
pub struct Author {
    model: OpenAiModel,
    messages: Vec<Message>,
    /// Model calls made so far.
    pub calls: u32,
}

impl std::fmt::Debug for Author {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Author")
            .field("calls", &self.calls)
            .finish_non_exhaustive()
    }
}

/// What the author answered.
#[derive(Debug, Clone)]
pub enum Authored {
    /// A flow to run.
    Flow(Value),
    /// The task is complete.
    Done,
}

impl Author {
    /// An author on `OpenRouter` using `OPENROUTER_API_KEY` and
    /// `TINYDESKTOP_LAB_MODEL`, primed with `guide`.
    ///
    /// # Errors
    ///
    /// Fails when the key is not exported.
    pub fn from_env(guide: &str) -> Result<Self, LabError> {
        let model_id = std::env::var("TINYDESKTOP_LAB_MODEL")
            .ok()
            .filter(|model| !model.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.to_owned());
        let spec = ProviderSpec::for_kind(ProviderKind::OpenRouter).with_model(model_id);
        let model = OpenAiModel::from_spec_env(spec)?
            .with_vision(true)
            .with_json_object_format(true);
        Ok(Self {
            model,
            messages: vec![Message::system(format!("{PROTOCOL}\n\n{guide}"))],
            calls: 0,
        })
    }

    /// Asks for the first flow for `brief`.
    ///
    /// # Errors
    ///
    /// Fails when the model call fails or never produces a valid flow.
    pub async fn begin(&mut self, host: &Host, brief: &str) -> Result<Authored, LabError> {
        self.messages.push(Message::user(format!("Task:\n{brief}")));
        self.answer(host).await
    }

    /// Reports a run's outcome and asks for the next flow, or `done`.
    ///
    /// # Errors
    ///
    /// Fails when the model call fails or never produces a valid flow.
    pub async fn continue_after(
        &mut self,
        host: &Host,
        summary: &str,
    ) -> Result<Authored, LabError> {
        self.messages.push(Message::user(format!(
            "The flow ran. What happened:\n{summary}\n\nIf the task is now complete, reply {{\"done\": true}}. \
Otherwise reply with a flow for only the remaining work."
        )));
        self.answer(host).await
    }

    /// Asks the vision model a yes/no question about a screenshot.
    ///
    /// # Errors
    ///
    /// Fails when the model call fails.
    pub async fn looks_true(
        &mut self,
        png_base64: &str,
        condition: &str,
    ) -> Result<bool, LabError> {
        self.calls += 1;
        let request = ModelRequest {
            messages: vec![Message::User(UserMessage {
                content: vec![
                    ContentBlock::Text(format!(
                        "Looking only at this screenshot, is this true? Answer with one word, yes or no.\n\n{condition}"
                    )),
                    ContentBlock::Image(ImageRef {
                        url: format!("data:image/png;base64,{png_base64}"),
                        mime_type: Some("image/png".to_owned()),
                    }),
                ],
            })],
            max_tokens: Some(8),
            ..ModelRequest::default()
        };
        let response = self.model.invoke(&(), request).await?;
        let text = Message::Assistant(response.message).text();
        Ok(text.trim().to_ascii_lowercase().starts_with("yes"))
    }

    async fn answer(&mut self, host: &Host) -> Result<Authored, LabError> {
        for _ in 0..=REPAIRS {
            let text = self.complete().await?;
            let value = match extract_json(&text) {
                Ok(value) => value,
                Err(error) => {
                    self.messages.push(Message::user(format!(
                        "That was not one JSON object ({error}). Reply with exactly one JSON object."
                    )));
                    continue;
                }
            };
            if value.get("done").and_then(Value::as_bool) == Some(true) {
                return Ok(Authored::Done);
            }
            let validation = host.validate(&value).await?;
            if validation.valid {
                return Ok(Authored::Flow(value));
            }
            self.messages.push(Message::user(format!(
                "That flow is invalid:\n- {}\nReply with the corrected flow only.",
                validation.errors.join("\n- ")
            )));
        }
        Err(std::io::Error::other("the author did not produce a valid flow").into())
    }

    async fn complete(&mut self) -> Result<String, LabError> {
        self.calls += 1;
        let request = ModelRequest {
            messages: self.messages.clone(),
            response_format: Some(ResponseFormat::JsonObject),
            temperature: Some(0.2),
            max_tokens: Some(4_000),
            ..ModelRequest::default()
        };
        let response = self.model.invoke(&(), request).await?;
        let message = Message::Assistant(response.message);
        let text = message.text();
        self.messages.push(message);
        Ok(text)
    }
}

/// The first JSON object in `text`, tolerating code fences and prose.
///
/// # Errors
///
/// Fails when no JSON object parses.
pub fn extract_json(text: &str) -> Result<Value, serde_json::Error> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Ok(value);
    }
    let start = trimmed.find('{').unwrap_or(0);
    let end = trimmed.rfind('}').map_or(trimmed.len(), |end| end + 1);
    serde_json::from_str(&trimmed[start..end.max(start)])
}
