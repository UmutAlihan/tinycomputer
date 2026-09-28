//! Making an option show when a `choose` step cannot find it: opening the
//! control, paging a calendar, or typing the option to filter a list.

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

    /// The next way to make `option` show after attempt `attempt` found
    /// nothing: open `what`, page a calendar, or type the option to filter.
    pub(super) async fn another_way(
        &mut self,
        log: &mut StepLog,
        attempt: usize,
        screen: &Screen,
        what: &str,
        option: &str,
        into_focus: bool,
    ) -> Result<(), Halt> {
        match attempt {
            0 => {
                // Revealing is one way among several; when it fails the
                // next attempt tries another rather than giving up.
                let revealed = self
                    .accomplish(
                        log,
                        &format!("open {what} so its options show"),
                        REVEAL_TURNS,
                    )
                    .await;
                match revealed {
                    Err(Halt::Failed(note)) => self.history.push(format!(
                        "could not open {what} ({note}); trying another way"
                    )),
                    other => {
                        other?;
                    }
                }
            }
            1 if looks_like_date(option) => self.page_to(log, option).await?,
            // An opened autocomplete holds the focus in its search input,
            // often unnamed; type there before anything moves the focus.
            1 if into_focus => self.type_into_focus(log, option).await?,
            2 if !looks_like_date(option) => {
                self.type_to_filter(log, screen, what, option).await?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Pages a calendar forward, one month at a time, until a control shows
    /// `date`; stops at [`MAX_MONTHS`] or where there is no next month.
    pub(super) async fn page_to(&mut self, log: &mut StepLog, date: &str) -> Result<(), Halt> {
        for _ in 0..MAX_MONTHS {
            let screen = self.look().await?;
            // Restricted to the same clickable, non-aggregating pool
            // `pick_option` selects from: a non-clickable calendar container
            // whose label lists every date in the month, or an unrelated
            // result elsewhere on the page, both "mention" the date without
            // being an actionable day, and stopping on either leaves the
            // step with nothing to press.
            if clickable(&screen.candidates)
                .iter()
                .any(|candidate| mentions(candidate, date) && !lists_more_than(candidate, date))
            {
                return Ok(());
            }
            let Some(next) = clickable(&screen.candidates)
                .into_iter()
                .find(|candidate| candidate.name.as_deref().is_some_and(is_next_month))
            else {
                return Ok(());
            };
            let clicked = next.clone();
            let reply = self
                .act(log, "click", Some(&next), move |backend| {
                    backend.execute(JevOperation::Click, Some(clicked), None)
                })
                .await?;
            if !reply.ok {
                return Ok(());
            }
        }
        Ok(())
    }

    /// Types `option` wherever the focus is.
    pub(super) async fn type_into_focus(&mut self, log: &mut StepLog, option: &str) -> Result<(), Halt> {
        let text = search_text(option);
        self.act(log, "type to filter", None, move |backend| {
            backend.execute(JevOperation::TypeText, None, Some(text))
        })
        .await?;
        log.filtered = true;
        self.history
            .push("typed into the focused field to filter it".to_owned());
        Ok(())
    }

    /// Types `option` into the search box of `what`, so an autocomplete
    /// lists it; nothing happens when no field takes text.
    pub(super) async fn type_to_filter(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        what: &str,
        option: &str,
    ) -> Result<(), Halt> {
        let fields = screen
            .candidates
            .iter()
            .filter(|candidate| {
                candidate
                    .available_actions
                    .iter()
                    .any(|action| action == "SetValue")
                    && !self.refused.contains(&element_kind(candidate))
            })
            .cloned()
            .collect::<Vec<_>>();
        let purpose = format!("the search box that filters the options of {what}");
        if let Some(grounded) = self.ground(log, screen, &purpose, &purpose, fields).await? {
            let app = self.app.clone();
            let target = grounded.candidate;
            let field = target.clone();
            let text = search_text(option);
            let reply = self
                .act(log, "type to filter", Some(&target), move |backend| {
                    deliver_text(&backend, &app, &field, &text)
                })
                .await?;
            self.typed.insert(element_kind(&target));
            log.filtered = true;
            if reply.ok {
                self.history
                    .push(format!("typed into {} to filter it", label(&target)));
            } else {
                self.refused.insert(element_kind(&target));
            }
        }
        Ok(())
    }

}
