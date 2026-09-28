//! Running one desktop action, charged to the budget and the step log.

use std::time::Instant;

use serde_json::{Value, json};
use tinycomputer_bus::{DesktopResponse, FlowActionRecord, FlowStopReason};

use super::{
    FlowRun, Halt, StepLog,
    backend::{AgentBackend, blocking},
    view::{Candidate, target_payload},
};
use crate::agentic::journal::millis;

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Runs one desktop action, charging it to the budget and the step log.
    pub(in crate::agentic::flow) async fn act<F>(
        &mut self,
        log: &mut StepLog,
        action: &str,
        target: Option<&Candidate>,
        call: F,
    ) -> Result<DesktopResponse, Halt>
    where
        F: FnOnce(B) -> DesktopResponse + Send + 'static,
    {
        if self.actions >= self.max_actions {
            return Err(Halt::Stop(FlowStopReason::ActionBudget));
        }
        self.actions = self.actions.saturating_add(1);
        let started = Instant::now();
        let reply = self.backend_call(call).await;
        let acted_ms = millis(started.elapsed());
        if let Some(url) = reply
            .data
            .as_ref()
            .and_then(|data| data.get("url"))
            .and_then(Value::as_str)
            .filter(|url| !url.is_empty())
        {
            self.location = Some(url.to_owned());
        }
        let note = match (&reply.error, &reply.data) {
            (Some(error), _) => error.code.clone(),
            (None, Some(data)) => data
                .get("path")
                .and_then(serde_json::Value::as_str)
                .map(|path| {
                    let verified = data
                        .get("verified")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(true);
                    format!("via {path}{}", if verified { "" } else { ", unverified" })
                })
                .unwrap_or_default(),
            (None, None) => String::new(),
        };
        log.actions.push(FlowActionRecord {
            action: action.to_owned(),
            target: target.map(target_payload),
            ok: reply.ok,
            note,
        });
        let settle_started = Instant::now();
        if reply.ok {
            // Let the surface finish reacting, so the next look sees what the
            // action did rather than the moment before it took effect.
            self.backend_call(|backend| {
                backend.settle();
                DesktopResponse::ok("settle", serde_json::json!({}))
            })
            .await;
        }
        self.runtime.journal.record("action", || {
            let record = log.actions.last();
            json!({
                "step": self.step,
                "action": action,
                "target": record.and_then(|record| record.target.as_ref()),
                "ok": reply.ok,
                "note": record.map(|record| record.note.as_str()),
                "wall_ms": acted_ms,
                "settle_ms": if reply.ok { millis(settle_started.elapsed()) } else { 0 },
            })
        });
        Ok(reply)
    }

    async fn backend_call<F>(&self, call: F) -> DesktopResponse
    where
        F: FnOnce(B) -> DesktopResponse + Send + 'static,
    {
        blocking(self.backend.clone(), call).await
    }
}
