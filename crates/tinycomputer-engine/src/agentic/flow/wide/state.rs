//! What every wide question sees: the screen as a digest, the run's working
//! memory, and the variables it has read so far.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use tinycomputer_bus::{FlowLoop, FlowStrategy};

use crate::agentic::flow::{
    AgentBackend, FlowRun,
    ask::{self},
    ledger::Context,
    view::{Digest, Rendering, Screen, digest},
};

use super::{COLLECTED_CHARS, DIGEST_BUDGET, MAX_COLLECTED};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Whether the run asks wide requests.
    pub(in crate::agentic::flow) fn wide(&self) -> bool {
        self.strategy == FlowStrategy::Wide
    }

    /// The state every wide question is asked against: where the run is,
    /// the screen as a digest, the static text, and the working memory.
    pub(in crate::agentic::flow) fn wide_state(&self, screen: &Screen, purpose: &str) -> Value {
        let mut state = ask::state(screen, purpose, &[], self.include_values);
        if let Value::Object(fields) = &mut state {
            fields.remove("recent_actions");
            if self.enabled(FlowLoop::Digest) {
                let digest = digest(screen);
                fields.remove("elements");
                let (relevance, distractions) = self.attention_for(&digest);
                fields.insert(
                    "screen".to_owned(),
                    digest.render(screen, &self.rendering(&relevance, &distractions)),
                );
            }
            fields.insert("memory".to_owned(), self.memory_view(purpose));
            if let Some(collected) = self.collected() {
                fields.insert("already_collected".to_owned(), collected);
            }
        }
        state
    }

    /// The step's survey answers for `digest`'s regions, by region id:
    /// empty when the step has not surveyed.
    pub(in crate::agentic::flow) fn attention_for(
        &self,
        digest: &Digest,
    ) -> (BTreeMap<String, f64>, BTreeSet<String>) {
        self.attention
            .as_ref()
            .filter(|attention| attention.step == self.step)
            .map(|attention| attention.for_digest(digest))
            .unwrap_or_default()
    }

    /// How a digest is rendered and ranked, given the survey's answers.
    pub(in crate::agentic::flow) fn rendering<'a>(
        &self,
        relevance: &'a BTreeMap<String, f64>,
        distractions: &'a BTreeSet<String>,
    ) -> Rendering<'a> {
        Rendering {
            include_values: self.include_values,
            budget: DIGEST_BUDGET,
            relevance: Some(relevance),
            distractions: Some(distractions),
        }
    }

    /// The memory section: finished steps, this step so far, what failed,
    /// the next step, variables read, and the budget left.
    fn memory_view(&self, purpose: &str) -> Value {
        let current = self
            .step
            .split('.')
            .next()
            .and_then(|top| top.parse::<usize>().ok())
            .unwrap_or(0);
        self.ledger.view(&Context {
            history: &self.history,
            now: purpose,
            next: self.outline.get(current).map(String::as_str),
            variables: self.read.clone(),
            budget_left: (
                self.max_actions.saturating_sub(self.actions),
                self.max_calls.saturating_sub(self.metrics.calls),
            ),
        })
    }

    /// What the run has saved so far, for Jev to remember across steps: the
    /// last [`MAX_COLLECTED`] variables read, each value clipped to
    /// [`COLLECTED_CHARS`], and an `extract`'s rows as their count and first
    /// row. `None` before anything is read. It goes through the same masking
    /// as the rest of the state.
    pub(in crate::agentic::flow) fn collected(&self) -> Option<Value> {
        let clip = |text: &str| -> String {
            let mut clipped: String = text.chars().take(COLLECTED_CHARS).collect();
            if text.chars().count() > COLLECTED_CHARS {
                clipped.push('…');
            }
            clipped
        };
        let saved = self
            .read
            .iter()
            .rev()
            .take(MAX_COLLECTED)
            .rev()
            .filter_map(|name| {
                let value = self.vars.get(name)?;
                let shown = match serde_json::from_str::<Vec<Vec<String>>>(value) {
                    Ok(rows) => format!(
                        "{} items; the first: {}",
                        rows.len(),
                        clip(&rows.first().map(|row| row.join(" · ")).unwrap_or_default())
                    ),
                    Err(_) => clip(value),
                };
                Some((name.clone(), Value::String(shown)))
            })
            .collect::<serde_json::Map<_, _>>();
        (!saved.is_empty()).then(|| {
            json!({"untrusted_accessibility_data": saved,
                   "note": "what earlier steps of this run read and saved, by variable name"})
        })
    }

    /// Notes that a step read text into the variable `name`.
    pub(in crate::agentic::flow) fn read_into(&mut self, name: &str) {
        if !self.read.iter().any(|read| read == name) {
            self.read.push(name.to_owned());
        }
    }
}
