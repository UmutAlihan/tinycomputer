//! The optional planner: a language model that turns a plain-language task
//! into a flow, and never acts.
//!
//! It is given the task, the flow guide, the *names* of the facts the caller
//! supplied, and which surfaces are available — never a fact's value and
//! never the screen. Its answer is validated with the same checker `RunFlow`
//! uses; an invalid flow goes back to the model with the errors, up to
//! [`REPAIRS`] times. Values the flow needs and no fact supplies come back as
//! questions for the caller.
//!
//! The model is behind [`LanguageModel`], so this logic is tested with
//! scripted answers; the `planner` feature adds an `OpenRouter` adapter.

#[cfg(feature = "planner")]
mod openrouter;

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;
use tinydesktop_bus::agent::{InputField, SurfaceKind, TaskPlan};
use tinydesktop_bus::{FLOW_GUIDE, Flow};

#[cfg(feature = "planner")]
pub use openrouter::{PLANNER_MODEL, PlannerConfig, open_router};

/// Validation repairs a plan gets.
pub const REPAIRS: usize = 2;

/// Who said a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The instructions.
    System,
    /// The caller's side.
    User,
    /// The model's side.
    Assistant,
}

/// One message in a planning conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    /// Who said it.
    pub role: Role,
    /// What was said.
    pub text: String,
}

impl Turn {
    fn new(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            text: text.into(),
        }
    }
}

/// The future a [`LanguageModel`] returns: its reply text, or why it failed.
pub type Completion = Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;

/// A chat model that answers a conversation with text.
pub trait LanguageModel: Send + Sync + 'static {
    /// The model's reply to `turns`, which it is asked to give as one JSON
    /// object.
    fn complete(&self, turns: &[Turn]) -> Completion;
}

const PROTOCOL: &str = "You plan tasks for a module that drives web pages and desktop \
applications for a person. You cannot see the screen and you do not know any site's or \
application's interface: you describe what should happen, in plain steps, following the \
guide below. Reply with exactly one JSON object and nothing else: a flow \
({\"app\": ..., \"steps\": [...]}). \
Use `browse` for anything on the web and `open` for a desktop application. \
Refer to the person's details only as ${name} variables: use the fact names you are given, \
and invent a clear name for any other detail the task needs, so the person can be asked for \
it. Never invent personal details. Never enter payment details: end any purchase or booking \
with a stop_before step for paying. Guard sending, deleting, publishing, or submitting with a \
stop_before step.";

/// Turns tasks into flows with a [`LanguageModel`].
#[derive(Clone)]
pub struct Planner {
    model: Arc<dyn LanguageModel>,
}

impl std::fmt::Debug for Planner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Planner").finish_non_exhaustive()
    }
}

impl Planner {
    /// A planner asking `model`.
    #[must_use]
    pub fn new(model: Arc<dyn LanguageModel>) -> Self {
        Self { model }
    }

    /// Drafts a flow for `task`.
    ///
    /// # Errors
    ///
    /// Why no valid flow came back: the model failed, or its answer stayed
    /// invalid after [`REPAIRS`] repairs.
    pub async fn plan(
        &self,
        task: &str,
        fact_names: &[String],
        surfaces: &[SurfaceKind],
    ) -> Result<TaskPlan, String> {
        let surfaces = if surfaces.is_empty() {
            "the web browser and desktop applications".to_owned()
        } else {
            surfaces
                .iter()
                .map(|surface| match surface {
                    SurfaceKind::Browser => "the web browser",
                    SurfaceKind::Desktop => "desktop applications",
                })
                .collect::<Vec<_>>()
                .join(" and ")
        };
        let facts = if fact_names.is_empty() {
            "none".to_owned()
        } else {
            fact_names
                .iter()
                .map(|name| format!("${{{name}}}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut turns = vec![
            Turn::new(Role::System, format!("{PROTOCOL}\n\n{FLOW_GUIDE}")),
            Turn::new(
                Role::User,
                format!("Task: {task}\n\nAvailable: {surfaces}.\nFacts you may use: {facts}."),
            ),
        ];
        let known = fact_names.iter().cloned().collect::<BTreeSet<_>>();
        let mut last = String::new();
        for _ in 0..=REPAIRS {
            let reply = self.model.complete(&turns).await?;
            turns.push(Turn::new(Role::Assistant, reply.clone()));
            let problem = match parse(&reply) {
                Ok(flow) => {
                    let errors = crate::agentic::check_flow(&flow, &known)
                        .errors
                        .into_iter()
                        .filter(|error| !error.contains("` is not defined in `vars`"))
                        .collect::<Vec<_>>();
                    if errors.is_empty() {
                        return Ok(plan_for(flow, &known));
                    }
                    format!("That flow is invalid:\n- {}", errors.join("\n- "))
                }
                Err(error) => format!("That was not a flow ({error})."),
            };
            last.clone_from(&problem);
            turns.push(Turn::new(
                Role::User,
                format!("{problem}\nReply with the corrected flow only, as one JSON object."),
            ));
        }
        Err(format!("the planner did not produce a valid flow: {last}"))
    }
}

fn plan_for(flow: Flow, known: &BTreeSet<String>) -> TaskPlan {
    let questions = crate::agentic::missing_inputs(&flow, known)
        .into_iter()
        .map(|name| InputField {
            why: format!("the plan uses ${{{name}}}"),
            kind: crate::task::input_kind(&name),
            name,
            options: Vec::new(),
        })
        .collect();
    let notes = if flow
        .steps
        .iter()
        .any(|step| matches!(step.action(), tinydesktop_bus::FlowAction::StopBefore(_)))
    {
        vec!["The plan stops before any irreversible or paid action.".to_owned()]
    } else {
        Vec::new()
    };
    TaskPlan {
        flow,
        questions,
        notes,
    }
}

/// The flow in a model's reply, tolerating code fences and prose around it.
fn parse(text: &str) -> Result<Flow, String> {
    let trimmed = text.trim();
    let start = trimmed.find('{').ok_or("no JSON object")?;
    let end = trimmed.rfind('}').ok_or("no JSON object")? + 1;
    let value: Value = serde_json::from_str(&trimmed[start..end.max(start)])
        .map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

#[cfg(test)]
mod test;
