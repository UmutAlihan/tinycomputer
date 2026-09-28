//! One function per step kind; `run` dispatches between them.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use tinycomputer_bus::{
    ChooseStep, FlowAction, FlowLoop, FlowStopReason, IfStep, JevOperation, PickStep, ReadStep,
    RepeatStep, StepOutcome,
};
use tinycomputer_core::surface::{Group, result_families, result_groups};
use tinycomputer_core::{Criterion, Record, rank};

use crate::workspace::BROWSER;

use super::{
    AgentBackend, Ended, FlowRun, Halt, StepLog,
    act::DONE,
    ask::{self, Questions, chosen, condition, numbered},
    backend::deliver_text,
    ground::Grounded,
    memory::{learn, remember},
    validate::{MAX_REPEAT, substitute_safe},
    view::{Candidate, Screen, element_kind, is_destructive, label, target_payload},
};

/// Turns a `do` step may spend.
const DO_TURNS: u32 = 8;
/// Turns spent opening the thing a `choose` step picks from.
const REVEAL_TURNS: u32 = 3;
/// Times `open` checks for a readable window, waiting between checks.
const WINDOW_CHECKS: u32 = 10;
/// Times a `wait_for` checks its condition, waiting between checks.
const WAIT_CHECKS: u32 = 10;
/// Most characters of a picked item's text kept in its variable.
const MAX_PICK_SUMMARY: usize = 400;
/// Least probability a `read` or `stop_before` target needs.
const LOCATE_FLOOR: f64 = 0.5;

/// Runs one step.
pub(super) async fn run<B: AgentBackend + Sync>(
    run: &mut FlowRun<'_, B>,
    log: &mut StepLog,
    action: &FlowAction,
    text: &str,
    path: &str,
) -> Result<Ended, Halt> {
    match action {
        FlowAction::Open(app) => run.open(log, app).await,
        FlowAction::Browse(url) => run.browse(log, url).await,
        FlowAction::Do(_) => run.accomplish(log, text, DO_TURNS).await,
        FlowAction::Enter(slots) => run.enter(log, &slots.0).await,
        FlowAction::Choose(choose) => run.choose(log, choose).await,
        FlowAction::Read(read) => run.read(log, read).await,
        FlowAction::Pick(pick) => run.pick(log, pick).await,
        FlowAction::Extract(read) => run.extract(read).await,
        FlowAction::Verify(_) => run.verify(log, text).await,
        FlowAction::WaitFor(_) => run.wait_for(log, text).await,
        FlowAction::StopBefore(_) => run.stop_before(log, text).await,
        FlowAction::RepeatUntil(repeat) => run.repeat(log, repeat, path).await,
        FlowAction::If(branch) => run.branch(log, branch, path).await,
    }
}

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    async fn open(&mut self, log: &mut StepLog, app: &str) -> Result<Ended, Halt> {
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
    async fn browse(&mut self, log: &mut StepLog, url: &str) -> Result<Ended, Halt> {
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
    async fn await_window(&self) -> bool {
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

    /// Judges one condition on the current screen.
    pub(super) async fn holds(
        &mut self,
        log: &mut StepLog,
        condition_text: &str,
    ) -> Result<f64, Halt> {
        log.used(FlowLoop::Completion);
        let screen = self.look().await?;
        let answers = self
            .ask(
                log,
                ask::request(
                    self.model(),
                    self.state(&screen, condition_text),
                    Questions::default()
                        .with("holds", condition(condition_text))
                        .with("negated", ask::negated(condition_text))
                        .with("coverage", ask::coverage(condition_text)),
                ),
            )
            .await?;
        let held = ask::combined(
            ask::calibrated(&answers, "holds", "negated"),
            ask::top_level(&answers, "coverage"),
        )
        .unwrap_or_default();
        log.confidence = Some(held);
        Ok(held)
    }

    async fn verify(&mut self, log: &mut StepLog, condition_text: &str) -> Result<Ended, Halt> {
        let held = self.holds(log, condition_text).await?;
        if held >= DONE {
            Ok(Ended::new(
                StepOutcome::Done,
                format!("holds (confidence {held:.2})"),
            ))
        } else {
            Err(Halt::Failed(format!(
                "does not hold (confidence {held:.2})"
            )))
        }
    }

    async fn wait_for(&mut self, log: &mut StepLog, condition_text: &str) -> Result<Ended, Halt> {
        for check in 0..WAIT_CHECKS {
            let held = self.holds(log, condition_text).await?;
            if held >= DONE {
                return Ok(Ended::new(
                    StepOutcome::Done,
                    format!("held after {} check(s)", check + 1),
                ));
            }
            self.act(log, "wait", None, |backend| {
                backend.execute(JevOperation::Wait, None, None)
            })
            .await?;
        }
        Err(Halt::Failed(format!(
            "still not true after {WAIT_CHECKS} checks"
        )))
    }

    async fn choose(&mut self, log: &mut StepLog, choose: &ChooseStep) -> Result<Ended, Halt> {
        // `what` and `option` are shown to Jev, so a secret is never
        // expanded into them; validation already rejects one there.
        let what = substitute_safe(&choose.what, &self.vars, &self.facts);
        let option = substitute_safe(&choose.option, &self.vars, &self.facts);
        self.pick_option(log, &what, &option, false, true).await
    }

    /// Picks `option` in `what` as a person works a list, an autocomplete
    /// box, or a date picker: take the option if it shows, else open the
    /// control, page a calendar forward to a date, or type the option to
    /// filter it. Only an element that shows the option is ever pressed, so
    /// a list that never shows it fails the step rather than picking another.
    ///
    /// A `private` option — a secret `enter` could not type into a field — is
    /// never written into a question: only elements that already show it are
    /// offered, so Jev sees nothing the page does not.
    ///
    /// `into_focus` lets it type the option wherever the focus is, as an
    /// opened autocomplete expects. `enter` turns that off for a slot with
    /// no field: the focus there is the field it just filled for another
    /// slot, and typing into it would spoil that value.
    pub(super) async fn pick_option(
        &mut self,
        log: &mut StepLog,
        what: &str,
        option: &str,
        private: bool,
        into_focus: bool,
    ) -> Result<Ended, Halt> {
        let purpose = if private {
            format!("pick the option in {what} that shows the value being entered")
        } else {
            format!("pick the option {option:?} in {what}")
        };
        for attempt in 0..4 {
            let screen = self.look().await?;
            if !private && let Some(ended) = self.made_already(&screen, what, option, attempt) {
                return Ok(ended);
            }
            let pool = clickable(&screen.candidates)
                .into_iter()
                .filter(|candidate| !is_destructive(candidate, &screen, &self.stop_before))
                .collect::<Vec<_>>();
            // A field that holds the typed option is where it was typed,
            // not one of the options it offers.
            let pool = closest(
                pool.into_iter()
                    .filter(|candidate| {
                        mentions(candidate, option)
                            && !editable(candidate)
                            && !lists_more_than(candidate, option)
                    })
                    .collect(),
            );
            let pool = within(pool, what);
            // Matches that all name one option leave nothing to judge; a
            // private option is never judged, since Jev is not told it. An
            // option no control names is a description ("the lowest fare"),
            // matched by Jev among the page's option controls.
            let grounded = if pool.is_empty() && !private {
                self.described(log, &screen, what, option).await?
            } else if private || one_option(&pool) {
                plainest(pool).map(|candidate| Grounded {
                    candidate,
                    confidence: 1.0,
                })
            } else {
                self.ground(log, &screen, &purpose, &purpose, pool).await?
            };
            if let Some(grounded) = &grounded
                && is_checked(&grounded.candidate)
            {
                self.history
                    .push(format!("{} is already chosen", label(&grounded.candidate)));
                self.remember_choice(&format!("chose {option:?} in {what}"));
                return Ok(Ended::new(
                    StepOutcome::AlreadyDone,
                    format!("{option:?} was already chosen"),
                ));
            }
            if let Some(grounded) = grounded {
                log.confidence = Some(grounded.confidence);
                let target = grounded.candidate;
                let clicked = target.clone();
                // A private option was matched because it already shows the
                // value being entered; logging its label would write that
                // value into history and the step's action record, exactly
                // what picking privately is meant to avoid. The redacted
                // copy still carries the real ref and role, so the click
                // itself is unaffected.
                let logged = if private {
                    redacted(&target)
                } else {
                    target.clone()
                };
                let reply = self
                    .act(log, "click", Some(&logged), move |backend| {
                        backend.execute(JevOperation::Click, Some(clicked), None)
                    })
                    .await?;
                if reply.ok {
                    learn(&mut self.learned, remember(&self.app, &purpose, &target));
                    self.history.push(if private {
                        format!("chose the value shown in {what}")
                    } else {
                        format!("chose an option with {}", label(&target))
                    });
                    self.remember_choice(&if private {
                        format!("chose the value in {what}")
                    } else {
                        format!("chose {option:?} in {what}")
                    });
                    return Ok(Ended::new(
                        StepOutcome::Done,
                        if private {
                            format!("chose the value in {what}")
                        } else {
                            format!("chose {option:?}")
                        },
                    ));
                }
            }
            self.another_way(log, attempt, &screen, what, option, into_focus)
                .await?;
        }
        Err(Halt::Failed(if private {
            format!("the value was not found in {what}")
        } else {
            format!("{option:?} was not found in {what}")
        }))
    }

    /// `AlreadyDone` when `screen` shows `option` chosen already: a checked
    /// option control, or — before this step acts, since what it types to
    /// filter a list would read back as the choice — a field holding it.
    fn made_already(
        &mut self,
        screen: &Screen,
        what: &str,
        option: &str,
        attempt: usize,
    ) -> Option<Ended> {
        let shown = already_chosen(screen, option)
            .map(|chosen| format!("{} is already chosen", label(&chosen)))
            .or_else(|| {
                (attempt == 0)
                    .then(|| already_holds(screen, option, &self.typed))
                    .flatten()
                    .map(|holder| format!("{} already shows {option:?}", label(&holder)))
            })?;
        self.history.push(shown);
        self.remember_choice(&format!("chose {option:?} in {what}"));
        Some(Ended::new(
            StepOutcome::AlreadyDone,
            format!("{option:?} was already chosen"),
        ))
    }

    /// The next way to make `option` show after attempt `attempt` found
    /// nothing: open `what`, page a calendar, or type the option to filter.
    async fn another_way(
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

    /// The option control on `screen` that fits `option` read as a
    /// description, by Jev; `None` when no control fits well enough.
    async fn described(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        what: &str,
        option: &str,
    ) -> Result<Option<Grounded>, Halt> {
        let options = clickable(&screen.candidates)
            .into_iter()
            .filter(|candidate| {
                is_one_option(candidate) && !is_destructive(candidate, screen, &self.stop_before)
            })
            .collect::<Vec<_>>();
        if options.is_empty() {
            return Ok(None);
        }
        let purpose = format!("pick the option in {what} that fits: {option}");
        Ok(self
            .ground(log, screen, &purpose, &purpose, options)
            .await?
            .filter(|grounded| grounded.confidence >= LOCATE_FLOOR))
    }

    /// Pages a calendar forward, one month at a time, until a control shows
    /// `date`; stops at [`MAX_MONTHS`] or where there is no next month.
    async fn page_to(&mut self, log: &mut StepLog, date: &str) -> Result<(), Halt> {
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
    async fn type_into_focus(&mut self, log: &mut StepLog, option: &str) -> Result<(), Halt> {
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
    async fn type_to_filter(
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

    async fn read(&mut self, log: &mut StepLog, read: &ReadStep) -> Result<Ended, Halt> {
        let what = substitute_safe(&read.what, &self.vars, &self.facts);
        let mut screen = self.look().await?;
        self.explore(&mut screen).await;
        let ordered = ask::ordered_nodes(&screen);
        let sources: Vec<(String, Value, String)> = screen
            .candidates
            .iter()
            .filter_map(|candidate| {
                // A rich-text area (a mail body, a web view) holds no value
                // of its own; its text is read from the ref-less nodes
                // `screen.text_nodes` kept for it, the same source
                // `field_contents` reads from for the state Jev already sees.
                let text = ask::rich_text(&ordered, candidate).or_else(|| readable(candidate))?;
                Some((
                    label(candidate),
                    json!({"untrusted_accessibility_data": {
                        "element": label(candidate),
                        "shows": if self.include_values { json!(text) } else { json!(format!("{} characters", text.chars().count())) },
                        "state": candidate.states.join(", "),
                    }}),
                    text,
                ))
            })
            .chain(screen.context.iter().map(|line| {
                (
                    line.clone(),
                    json!({"untrusted_accessibility_data": {"text": line}}),
                    line.clone(),
                )
            }))
            .collect();
        if sources.is_empty() {
            return Err(Halt::Failed(format!("nothing readable shows {what}")));
        }
        // A screen with more than a page of sources is read a page at a
        // time, rather than truncated: a valid target past the cutoff must
        // still be found, not permanently dropped because of where it sits.
        for page in sources.chunks(ask::MAX_READ_SOURCES) {
            let keys = numbered(page.len());
            log.used(FlowLoop::Narrowing);
            let answers = self
                .ask(
                    log,
                    ask::request(
                        self.model(),
                        self.state(&screen, &format!("read {what}")),
                        Questions::default().with(
                            "source",
                            ask::options(
                                json!({
                                    "task": "Choose the piece of text on screen that shows this.",
                                    "what": what,
                                }),
                                keys.iter().cloned().zip(
                                    page.iter().map(|(_, description, _)| description.clone()),
                                ),
                            ),
                        ),
                    ),
                )
                .await?;
            let Some((choice, confidence)) =
                chosen(&answers, "source").filter(|(_, confidence)| *confidence >= LOCATE_FLOOR)
            else {
                continue;
            };
            let Some((source, _, text)) = keys
                .iter()
                .position(|key| *key == choice)
                .and_then(|index| page.get(index))
            else {
                continue;
            };
            log.confidence = Some(confidence);
            self.vars.insert(read.into.clone(), text.clone());
            self.read_into(&read.into);
            self.history
                .push(format!("read {what} from {source} into {}", read.into));
            return Ok(Ended::new(
                StepOutcome::Done,
                format!(
                    "read {} characters into {}",
                    text.chars().count(),
                    read.into
                ),
            ));
        }
        Err(Halt::Failed(format!(
            "no text on screen clearly shows {what}"
        )))
    }

    /// Picks the best of a list of results by `pick.by`, stores its text,
    /// and opens it. A criterion over prices, times, durations, or stops is
    /// ranked exactly; anything else is judged by Jev among the records.
    async fn pick(&mut self, log: &mut StepLog, pick: &PickStep) -> Result<Ended, Halt> {
        let from = substitute_safe(&pick.from, &self.vars, &self.facts);
        let by = substitute_safe(&pick.by, &self.vars, &self.facts);
        let mut screen = self.look().await?;
        self.explore(&mut screen).await;
        let families = result_families(&screen);
        if families.is_empty() {
            return Err(Halt::Failed(format!("no list of {from} is showing")));
        }
        // A page can repeat several things (a strip of dates above the
        // flights); a measurable criterion ranks the first list that has
        // the measure, and judgement falls to the longest.
        let ranked = Criterion::parse(&by).and_then(|criterion| {
            families.iter().find_map(|groups| {
                rank(&records_of(groups), criterion).map(|order| (groups, order[0]))
            })
        });
        let (groups, best, how) = if let Some((groups, best)) = ranked {
            (groups, best, "ranked")
        } else {
            let groups = &families[0];
            let best = self.judge_pick(log, &screen, &from, &by, groups).await?;
            (groups, best, "judged")
        };
        let group = &groups[best];
        let summary: String = group
            .fields
            .join(" · ")
            .chars()
            .take(MAX_PICK_SUMMARY)
            .collect();
        if let Some(into) = &pick.into {
            self.vars.insert(into.clone(), summary.clone());
            self.read_into(into);
        }
        let Some(primary) = group.primary.clone() else {
            return Err(Halt::Failed(format!(
                "the picked item has nothing to open: {summary}"
            )));
        };
        if is_destructive(&primary, &screen, &self.stop_before) {
            return Err(Halt::Failed(format!(
                "refused to press {} to pick an item",
                label(&primary)
            )));
        }
        let reply = self
            .press_uncovering(log, "click", &primary, JevOperation::Click)
            .await?;
        if !reply.ok {
            let why = reply.error.as_ref().map_or_else(
                || "no reason given".to_owned(),
                |error| error.message.clone(),
            );
            return Err(Halt::Failed(format!(
                "could not open the picked item ({why}): {summary}"
            )));
        }
        self.history
            .push(format!("picked {summary} ({how} by {by})"));
        self.remember_choice(&format!("picked from {from} by {by}: {summary}"));
        Ok(Ended::new(
            StepOutcome::Done,
            format!("picked {summary} ({how} by {by}, out of {})", groups.len()),
        ))
    }

    /// Stores every item of the list showing as JSON rows of their text.
    async fn extract(&mut self, read: &ReadStep) -> Result<Ended, Halt> {
        let what = substitute_safe(&read.what, &self.vars, &self.facts);
        let mut screen = self.look().await?;
        self.explore(&mut screen).await;
        let groups = result_groups(&screen);
        if groups.is_empty() {
            return Err(Halt::Failed(format!("no list of {what} is showing")));
        }
        let rows = groups
            .iter()
            .map(|group| group.fields.clone())
            .collect::<Vec<_>>();
        self.vars.insert(
            read.into.clone(),
            serde_json::to_string(&rows).unwrap_or_default(),
        );
        self.read_into(&read.into);
        self.history.push(format!(
            "extracted {} items of {what} into {}",
            rows.len(),
            read.into
        ));
        Ok(Ended::new(
            StepOutcome::Done,
            format!("extracted {} items into {}", rows.len(), read.into),
        ))
    }

    /// Asks Jev which record best meets `by`, among the first
    /// [`ask::MAX_READ_SOURCES`]-sized page of them.
    async fn judge_pick(
        &mut self,
        log: &mut StepLog,
        screen: &super::view::Screen,
        from: &str,
        by: &str,
        groups: &[Group],
    ) -> Result<usize, Halt> {
        let shown = &groups[..groups.len().min(ask::MAX_READ_SOURCES)];
        let keys = numbered(shown.len());
        log.used(FlowLoop::Narrowing);
        let answers = self
            .ask(
                log,
                ask::request(
                    self.model(),
                    self.state(screen, &format!("pick from {from} by {by}")),
                    Questions::default().with(
                        "record",
                        ask::options(
                            json!({
                                "task": "Choose the item in this list that best meets the criterion.",
                                "list": from,
                                "criterion": by,
                            }),
                            keys.iter().cloned().zip(shown.iter().map(|group| {
                                json!({"untrusted_accessibility_data": {"item": group.fields}})
                            })),
                        ),
                    ),
                ),
            )
            .await?;
        let Some((choice, confidence)) =
            chosen(&answers, "record").filter(|(_, confidence)| *confidence >= LOCATE_FLOOR)
        else {
            return Err(Halt::Failed(format!(
                "no item in {from} clearly meets {by}"
            )));
        };
        log.confidence = Some(confidence);
        keys.iter()
            .position(|key| *key == choice)
            .ok_or_else(|| Halt::Failed(format!("no item in {from} clearly meets {by}")))
    }

    async fn stop_before(&mut self, log: &mut StepLog, action: &str) -> Result<Ended, Halt> {
        let purpose = format!("perform: {action}");
        // Asked to "perform: paying", Jev weighs the request against the
        // brief's own rule to stop before paying and hesitates (measured:
        // 0.44 on the Pay button); asked to find it without pressing it,
        // which is all a gated step does, it answers 1.0.
        let question = if self.allow_destructive {
            purpose.clone()
        } else {
            format!("find, without pressing it, the control that would perform: {action}")
        };
        let screen = self.look().await?;
        let pool = clickable(&screen.candidates);
        let Some(grounded) = self
            .ground(log, &screen, &question, &purpose, pool)
            .await?
            .filter(|grounded| grounded.confidence >= LOCATE_FLOOR)
        else {
            return Err(Halt::Failed(format!(
                "the control that performs {action:?} was not found"
            )));
        };
        log.confidence = Some(grounded.confidence);
        let target = grounded.candidate;
        if !self.allow_destructive {
            self.pending = Some(target_payload(&target));
            self.history.push(format!(
                "found {} for {action:?} and stopped in front of it",
                label(&target)
            ));
            return Err(Halt::Stop(FlowStopReason::StoppedBeforeDestructive));
        }
        let clicked = target.clone();
        let reply = self
            .act(log, "click (irreversible)", Some(&target), move |backend| {
                backend.execute(JevOperation::Click, Some(clicked), None)
            })
            .await?;
        if !reply.ok {
            return Err(Halt::Failed(format!(
                "{} could not be pressed",
                label(&target)
            )));
        }
        learn(&mut self.learned, remember(&self.app, &purpose, &target));
        let happened = self.holds(log, &format!("{action} has happened")).await?;
        if happened >= DONE {
            Ok(Ended::new(
                StepOutcome::Done,
                format!("performed {action:?}"),
            ))
        } else {
            Err(Halt::Failed(format!(
                "pressed {} but {action:?} is not visibly done (confidence {happened:.2})",
                label(&target)
            )))
        }
    }

    async fn repeat(
        &mut self,
        log: &mut StepLog,
        repeat: &RepeatStep,
        path: &str,
    ) -> Result<Ended, Halt> {
        let condition_text = substitute_safe(&repeat.condition, &self.vars, &self.facts);
        for round in 0..repeat.max.min(MAX_REPEAT) {
            if self.holds(log, &condition_text).await? >= DONE {
                return Ok(Ended::new(
                    StepOutcome::Done,
                    format!("held after {round} round(s)"),
                ));
            }
            self.run_steps(&repeat.steps, format!("{path}.r{}", round + 1))
                .await?;
            // The last child left `self.step` at its own nested path; restore
            // it before the next `holds` check so that call, and the final
            // one below on the last round, are traced to this repeat_until
            // step rather than misattributed to the child that just ran.
            path.clone_into(&mut self.step);
        }
        if self.holds(log, &condition_text).await? >= DONE {
            return Ok(Ended::new(StepOutcome::Done, "held after the last round"));
        }
        Err(Halt::Failed(format!(
            "still not true after {} round(s)",
            repeat.max.min(MAX_REPEAT)
        )))
    }

    async fn branch(
        &mut self,
        log: &mut StepLog,
        branch: &IfStep,
        path: &str,
    ) -> Result<Ended, Halt> {
        let condition_text = substitute_safe(&branch.condition, &self.vars, &self.facts);
        let held = self.holds(log, &condition_text).await?;
        let (steps, taken) = if held >= DONE {
            (&branch.then, "then")
        } else {
            (&branch.otherwise, "else")
        };
        self.run_steps(steps, path.to_owned()).await?;
        Ok(Ended::new(
            StepOutcome::Done,
            format!("took the {taken} branch (confidence {held:.2})"),
        ))
    }
}

/// The months a date picker is paged forward at most.
const MAX_MONTHS: usize = 12;

/// Month names, as a date option spells them.
const MONTHS: &[&str] = &[
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// Whether `option` names a calendar day: a month name and a day number that
/// is a real day of that month (a year, when given, decides February's 28th
/// against its 29th). `February 31` or `April 31` names no such day, and is
/// read as ordinary autocomplete text instead of taking the calendar path.
pub(super) fn looks_like_date(option: &str) -> bool {
    let lower = option.to_lowercase();
    let words = lower
        .split(|character: char| !character.is_alphanumeric())
        .collect::<Vec<_>>();
    let Some(month) = MONTHS.iter().position(|month| words.contains(month)) else {
        return false;
    };
    let Some(day) = words
        .iter()
        .find_map(|word| word.parse::<u8>().ok().filter(|day| (1..=31).contains(day)))
    else {
        return false;
    };
    let year = words.iter().find_map(|word| {
        word.parse::<u16>()
            .ok()
            .filter(|year| (1900..=2100).contains(year))
    });
    day <= days_in_month(month, year)
}

/// How many days `month` (0 = January, from [`MONTHS`]) has; February is
/// taken as 29 when no `year` narrows it, so a bare "29 February" is still
/// treated as a date worth paging to.
fn days_in_month(month: usize, year: Option<u16>) -> u8 {
    match month {
        0 | 2 | 4 | 6 | 7 | 9 | 11 => 31,
        3 | 5 | 8 | 10 => 30,
        _ => {
            if year.is_none_or(is_leap_year) {
                29
            } else {
                28
            }
        }
    }
}

/// Whether `year` is a leap year in the Gregorian calendar.
fn is_leap_year(year: u16) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Whether a control's label says only that it shows the next month; a
/// date field whose label lists the whole calendar says much more.
fn is_next_month(name: &str) -> bool {
    let words = plain(name);
    words.contains("next month") && words.split(' ').count() <= 4
}

/// The matches whose labels say little besides the option: a container
/// whose label strings together everything inside it (a calendar button
/// named with every day of the month) is dropped when a plainer match exists.
pub(super) fn closest(matches: Vec<Candidate>) -> Vec<Candidate> {
    let length = |candidate: &Candidate| candidate.name.as_deref().map_or(0, str::len);
    let Some(shortest) = matches.iter().map(length).min() else {
        return matches;
    };
    matches
        .into_iter()
        .filter(|candidate| length(candidate) <= shortest.saturating_mul(3).max(shortest + 40))
        .collect()
}

/// Lower-case words joined by single spaces, so `Sunday, 18 October` and
/// `sunday 18 october` compare equal.
fn plain(text: &str) -> String {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The day, month, and year (when given) a date option names, as words.
fn date_words(option: &str) -> Vec<String> {
    plain(option)
        .split(' ')
        .filter(|word| {
            MONTHS.contains(word)
                || word.parse::<u16>().is_ok_and(|number| {
                    (1..=31).contains(&number) || (1900..=2100).contains(&number)
                })
        })
        .map(str::to_owned)
        .collect()
}

/// Each card's text as a record, its fields numbered in reading order.
fn records_of(groups: &[Group]) -> Vec<Record> {
    groups
        .iter()
        .map(|group| Record {
            fields: group
                .fields
                .iter()
                .enumerate()
                .map(|(index, text)| (format!("field {index}"), text.clone()))
                .collect(),
        })
        .collect()
}

/// Words beyond the option's own that a label may carry and still be the
/// option ("Srinagar, SXR Srinagar International Airport").
const OPTION_EXTRA_WORDS: usize = 12;

/// Roles of a control that is one option however much its label says: a
/// fare card's radio names its price, baggage, and rules, and is still just
/// "Saver".
const ONE_OPTION_ROLES: &[&str] = &[
    "radio",
    "radiobutton",
    "option",
    "menuitemradio",
    "checkbox",
];

/// Whether a control is checked or selected already.
fn is_checked(candidate: &Candidate) -> bool {
    candidate
        .states
        .iter()
        .any(|state| state == "checked" || state == "selected")
}

fn is_one_option(candidate: &Candidate) -> bool {
    ONE_OPTION_ROLES
        .iter()
        .any(|role| candidate.role.eq_ignore_ascii_case(role))
}

/// The option control on `screen` that is already checked or selected and
/// whose label starts with `option`: there is nothing to choose.
pub(super) fn already_chosen(screen: &Screen, option: &str) -> Option<Candidate> {
    let wanted = plain(option);
    if wanted.is_empty() {
        return None;
    }
    screen
        .candidates
        .iter()
        .find(|candidate| {
            is_one_option(candidate)
                && is_checked(candidate)
                && candidate
                    .name
                    .as_deref()
                    .is_some_and(|name| plain(name).starts_with(&wanted))
        })
        .cloned()
}

/// The field on `screen` that already shows exactly `option` as its value,
/// when the flow did not type it there (`typed`, by `element_kind`): a
/// passengers box reading "1 Adult" has that choice made, and pressing the
/// stepper beside it, whose label also mentions "1 Adult", would change it.
pub(super) fn already_holds(
    screen: &Screen,
    option: &str,
    typed: &BTreeSet<String>,
) -> Option<Candidate> {
    let wanted = plain(option);
    if wanted.is_empty() {
        return None;
    }
    screen
        .candidates
        .iter()
        .find(|candidate| {
            candidate
                .value
                .as_ref()
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| plain(value) == wanted)
                && !typed.contains(&element_kind(candidate))
        })
        .cloned()
}

/// Roles a page marks as the one chosen among its siblings.
const SELECTABLE_ROLES: &[&str] = &["tab", "radio", "radiobutton", "option", "menuitemradio"];

/// Why `screen` plainly shows `option` not chosen: the tab or radio named
/// exactly `option` is not selected while a sibling of its kind is; or, when
/// the step typed to filter a list (`filtered`), the option it pressed is
/// still offered there unselected — a press that took closes the list or
/// marks the option. `None` when nothing on screen settles it, and Jev is
/// asked instead.
pub(super) fn left_unchosen(screen: &Screen, option: &str, filtered: bool) -> Option<String> {
    let wanted = plain(option);
    if wanted.is_empty() {
        return None;
    }
    let selectable = |candidate: &&Candidate| {
        SELECTABLE_ROLES
            .iter()
            .any(|role| candidate.role.eq_ignore_ascii_case(role))
    };
    let asked = screen
        .candidates
        .iter()
        .filter(selectable)
        .find(|candidate| {
            candidate
                .name
                .as_deref()
                .is_some_and(|name| plain(name) == wanted)
        })?;
    if is_checked(asked) {
        return None;
    }
    if filtered && asked.role.eq_ignore_ascii_case("option") {
        return Some(format!("{} is still offered, unselected", label(asked)));
    }
    let other = screen
        .candidates
        .iter()
        .filter(selectable)
        .find(|candidate| {
            candidate.role == asked.role
                && container(&candidate.path) == container(&asked.path)
                && is_checked(candidate)
        })?;
    Some(format!(
        "{} is not selected; {} is",
        label(asked),
        label(other)
    ))
}

/// The ancestors siblings share: `path` without the numbered items it ends
/// in, since each tab of a strip sits in its own `listitem #n`.
fn container(path: &[String]) -> &[String] {
    let numbered = |segment: &&String| {
        segment
            .rsplit_once(" #")
            .is_some_and(|(_, number)| number.parse::<u32>().is_ok())
    };
    let kept = path.len() - path.iter().rev().take_while(numbered).count();
    &path[..kept]
}

/// Whether a label says far more than the option: a control whose name
/// strings together a whole list (recent searches, every day of a month)
/// mentions the option without being it. An option control is never such a
/// list, however long its label.
pub(super) fn lists_more_than(candidate: &Candidate, option: &str) -> bool {
    if is_one_option(candidate) {
        return false;
    }
    let words = |text: &str| {
        plain(text)
            .split(' ')
            .filter(|word| !word.is_empty())
            .count()
    };
    candidate
        .name
        .as_deref()
        .is_some_and(|name| words(name) > words(option) + OPTION_EXTRA_WORDS)
}

/// Whether an element takes typed text.
fn editable(candidate: &Candidate) -> bool {
    candidate
        .available_actions
        .iter()
        .any(|action| action == "SetValue")
}

/// Whether every match carries the same label, as a day's button and its
/// grid cell do.
fn one_option(matches: &[Candidate]) -> bool {
    let mut labels = matches
        .iter()
        .map(|candidate| plain(candidate.name.as_deref().unwrap_or_default()));
    labels
        .next()
        .is_some_and(|first| labels.all(|label| label == first))
}

/// The match to press without judgement: a button, option, or link before a
/// cell or container, then the shortest label.
fn plainest(matches: Vec<Candidate>) -> Option<Candidate> {
    let rank = |candidate: &Candidate| {
        let role = match candidate.role.as_str() {
            "button" | "option" | "menuitem" | "link" | "radio" => 0,
            _ => 1,
        };
        (role, candidate.name.as_deref().map_or(0, str::len))
    };
    matches.into_iter().min_by_key(rank)
}

/// Whether an element shows `option` in its name, value, or description. A
/// date matches by its day, month, and year, whatever the weekday or order
/// (`Sunday, 18 October 2026` shows `18 October 2026`).
fn mentions(candidate: &Candidate, option: &str) -> bool {
    let wanted = plain(option);
    let date = looks_like_date(option).then(|| date_words(option));
    !wanted.is_empty()
        && [
            candidate.name.clone(),
            candidate.description.clone(),
            candidate.value.as_ref().map(ToString::to_string),
        ]
        .into_iter()
        .flatten()
        .any(|text| {
            let shown = format!(" {} ", plain(&text));
            match &date {
                Some(words) => words
                    .iter()
                    .all(|word| shown.contains(&format!(" {word} "))),
                // "Srinagar (SXR)" is the "Srinagar ... Airport SXR" row: the
                // exact phrase, or else every one of its words.
                None => {
                    shown.contains(&format!(" {wanted} "))
                        || wanted
                            .split(' ')
                            .all(|word| shown.contains(&format!(" {word} ")))
                }
            }
        })
}

/// What to type to find `option` in a search box: its name before any
/// qualifier, so "Srinagar (SXR)" searches for "Srinagar" — a box matching
/// on the name would find nothing for the whole of it.
pub(super) fn search_text(option: &str) -> String {
    let name = option
        .split(['(', ','])
        .next()
        .map(str::trim)
        .unwrap_or_default();
    if name.is_empty() {
        option.trim().to_owned()
    } else {
        name.to_owned()
    }
}

/// The matches in `pool` inside the region `what` names, or all of them
/// when the page places none there.
///
/// `what` names the control the option belongs to (a seat picker, a
/// destination search); a page rarely echoes that description on the
/// option's own label, so an unrelated control elsewhere that happens to
/// share the option's text must not qualify. Some pages carry no region
/// text at all, and narrowing then would drop every real option.
fn within(pool: Vec<Candidate>, what: &str) -> Vec<Candidate> {
    let regional = pool
        .iter()
        .filter(|candidate| in_region(candidate, what))
        .cloned()
        .collect::<Vec<_>>();
    if regional.is_empty() { pool } else { regional }
}

/// Whether `candidate` sits inside — or itself names — the region `what`
/// describes. A page rarely echoes a description such as "the outbound
/// flight list" on an option's own label, so this also checks the option's
/// ancestor labels (`path`), which the snapshot records outermost first.
pub(super) fn in_region(candidate: &Candidate, what: &str) -> bool {
    let wanted = plain(what);
    if wanted.is_empty() {
        return true;
    }
    let names = |text: &str| format!(" {} ", plain(text)).contains(&format!(" {wanted} "));
    [candidate.name.as_deref(), candidate.description.as_deref()]
        .into_iter()
        .flatten()
        .any(names)
        || candidate.path.iter().any(|ancestor| names(ancestor))
}

/// A copy of `candidate` with its shown text stripped, for logging a private
/// choice without writing the value it displayed into history.
pub(super) fn redacted(candidate: &Candidate) -> Candidate {
    Candidate {
        name: None,
        description: None,
        value: None,
        ..candidate.clone()
    }
}

/// Elements that can be pressed.
fn clickable(candidates: &[Candidate]) -> Vec<Candidate> {
    candidates
        .iter()
        .filter(|candidate| {
            candidate
                .available_actions
                .iter()
                .any(|action| action == "Click")
        })
        .cloned()
        .collect()
}

/// The text an element shows: its value, else its name.
///
/// A control's numeric value (a radio button's `1`) says less than its name,
/// so a named control with a number for a value reads as its name.
pub(super) fn readable(candidate: &Candidate) -> Option<String> {
    let value = candidate
        .value
        .as_ref()
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let numeric = value.is_some_and(|value| value.parse::<f64>().is_ok());
    match (value, candidate.name.as_deref()) {
        (Some(_), Some(name)) if numeric && !name.trim().is_empty() => Some(name.to_owned()),
        (Some(value), _) => Some(value.to_owned()),
        (None, Some(name)) if !name.trim().is_empty() => Some(name.to_owned()),
        _ => None,
    }
}
