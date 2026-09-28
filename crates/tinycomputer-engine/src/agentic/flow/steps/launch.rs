//! The `open` and `browse` steps: launching an application or a page and
//! waiting for it to show a readable window.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use tinycomputer_bus::{
    ChooseStep, FlowAction, FlowLoop, FlowStopReason, IfStep, JevOperation, PickStep, ReadStep,
    RepeatStep, StepOutcome,
};
use tinycomputer_core::surface::{Group, result_families};
use tinycomputer_core::{Criterion, Record, rank};

use crate::workspace::BROWSER;

use crate::agentic::flow::{
    Ended, FlowRun, Halt, StepLog,
    act::{DONE, SCREEN_VIEW},
    ask::{self, Questions, chosen, condition, numbered},
    backend::{AgentBackend, deliver_text},
    escalate::Belief,
    ground::Grounded,
    memory::{learn, remember},
    validate::{MAX_REPEAT, substitute_safe},
    view::{Candidate, Screen, element_kind, is_destructive, label, target_payload},
};

use super::*;

impl<B: AgentBackend + Sync> FlowRun<'_, B> {

    pub(super) async fn open(&mut self, log: &mut StepLog, app: &str) -> Result<Ended, Halt> {
        // The launched application becomes `self.app`, and this step's note
        // joins `history` — both reach Jev on a later step — so a fact here
        // is rejected by validation and never expanded, same as everywhere
        // else but an `enter` value.
        let app = substitute_safe(app, &self.vars, &self.facts);
        self.app.clone_from(&app);
        let launched = app.clone();
        let reply = self
            .act(log, &format!("launch {app}"), None, move |backend| {
                backend.launch(&launched)
            })
            .await?;
        if reply.ok {
            let ready = self.await_window().await;
            Ok(Ended::new(
                StepOutcome::Done,
                if ready {
                    format!("{app} is open")
                } else {
                    format!("{app} is open but shows no readable window yet")
                },
            ))
        } else {
            Err(Halt::Failed(format!(
                "{app} could not be opened: {}",
                reply
                    .error
                    .as_ref()
                    .map_or("unknown error", |error| error.code.as_str())
            )))
        }
    }

    /// Opens `url` in the browser and moves the flow onto the page.
    pub(super) async fn browse(&mut self, log: &mut StepLog, url: &str) -> Result<Ended, Halt> {
        // Same reasoning as `open`: the address ends up in this step's note
        // in `history`, so it goes through the fact-safe substitution too.
        let url = substitute_safe(url, &self.vars, &self.facts);
        BROWSER.clone_into(&mut self.app);
        let address = url.clone();
        let reply = self
            .act(log, &format!("browse {url}"), None, move |backend| {
                let launched = backend.launch(BROWSER);
                if launched.ok {
                    backend.navigate(&address)
                } else {
                    launched
                }
            })
            .await?;
        if !reply.ok {
            return Err(Halt::Failed(format!(
                "{url} could not be opened: {}",
                reply
                    .error
                    .as_ref()
                    .map_or("unknown error", |error| error.code.as_str())
            )));
        }
        let title = reply
            .data
            .as_ref()
            .and_then(|data| data.get("title"))
            .and_then(serde_json::Value::as_str)
            .filter(|title| !title.is_empty())
            .map_or_else(String::new, |title| format!(" ({title})"));
        let ready = self.await_window().await;
        Ok(Ended::new(
            StepOutcome::Done,
            if ready {
                format!("{url} is open{title}")
            } else {
                format!("{url} is open but shows no readable page yet")
            },
        ))
    }

    /// Waits for the application to show a readable window, as a freshly
    /// launched one takes a moment to.
    pub(super) async fn await_window(&self) -> bool {
        for _ in 0..WINDOW_CHECKS {
            if super::backend::observe_async(
                self.backend.clone(),
                self.app.clone(),
                None,
                super::view::Depth::Skeleton,
            )
            .await
            .is_ok()
            {
                return true;
            }
            let _ = super::backend::blocking(self.backend.clone(), |backend| {
                backend.execute(JevOperation::Wait, None, None)
            })
            .await;
        }
        false
    }

}
