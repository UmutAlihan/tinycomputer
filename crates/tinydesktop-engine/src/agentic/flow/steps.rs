//! One function per step kind; `run` dispatches between them.

use serde_json::{Value, json};
use tinydesktop_bus::{
    ChooseStep, FlowAction, FlowLoop, FlowStopReason, IfStep, JevOperation, PickStep, ReadStep,
    RepeatStep, StepOutcome,
};
use tinydesktop_core::surface::{Group, result_groups};
use tinydesktop_core::{Criterion, Record, rank};

use crate::workspace::BROWSER;

use super::{
    AgentBackend, Ended, FlowRun, Halt, StepLog,
    act::DONE,
    ask::{self, Questions, chosen, condition, numbered},
    memory::{learn, remember},
    validate::{MAX_REPEAT, substitute},
    backend::deliver_text,
    view::{Candidate, Screen, is_destructive, label, target_payload},
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
        let app = substitute(app, &self.vars);
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
        let url = substitute(url, &self.vars);
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
        let what = substitute(&choose.what, &self.vars);
        let option = substitute(&choose.option, &self.vars);
        self.pick_option(log, &what, &option, false).await
    }

    /// Picks `option` in `what` as a person works a list, an autocomplete
    /// box, or a date picker: take the option if it shows, else open the
    /// control, page a calendar forward to a date, type the option to filter
    /// it, and only then (for a public option) judge every control.
    ///
    /// A `private` option — a value `enter` could not type into a field — is
    /// never written into a question: only elements that already show it are
    /// offered, so Jev sees nothing the page does not.
    pub(super) async fn pick_option(
        &mut self,
        log: &mut StepLog,
        what: &str,
        option: &str,
        private: bool,
    ) -> Result<Ended, Halt> {
        let purpose = if private {
            format!("pick the option in {what} that shows the value being entered")
        } else {
            format!("pick the option {option:?} in {what}")
        };
        let attempts = if private { 3 } else { 4 };
        for attempt in 0..attempts {
            let screen = self.look().await?;
            let pool = clickable(&screen.candidates)
                .into_iter()
                .filter(|candidate| !is_destructive(candidate, &screen, &self.stop_before))
                .collect::<Vec<_>>();
            let pool = if attempt == 3 {
                pool
            } else {
                closest(
                    pool.into_iter()
                        .filter(|candidate| mentions(candidate, option))
                        .collect(),
                )
            };
            if let Some(grounded) = self.ground(log, &screen, &purpose, &purpose, pool).await? {
                log.confidence = Some(grounded.confidence);
                let target = grounded.candidate;
                let clicked = target.clone();
                let reply = self
                    .act(log, "click", Some(&target), move |backend| {
                        backend.execute(JevOperation::Click, Some(clicked), None)
                    })
                    .await?;
                if reply.ok {
                    learn(&mut self.learned, remember(&self.app, &purpose, &target));
                    self.history.push(format!("chose an option with {}", label(&target)));
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
            match attempt {
                0 => {
                    self.accomplish(
                        log,
                        &format!("open {what} so its options show"),
                        REVEAL_TURNS,
                    )
                    .await?;
                }
                1 if looks_like_date(option) => self.page_to(log, option).await?,
                1 => self.type_to_filter(log, &screen, what, option).await?,
                _ => {}
            }
        }
        Err(Halt::Failed(if private {
            format!("the value was not found in {what}")
        } else {
            format!("{option:?} was not found in {what}")
        }))
    }

    /// Pages a calendar forward, one month at a time, until a control shows
    /// `date`; stops at [`MAX_MONTHS`] or where there is no next month.
    async fn page_to(&mut self, log: &mut StepLog, date: &str) -> Result<(), Halt> {
        for _ in 0..MAX_MONTHS {
            let screen = self.look().await?;
            if screen
                .candidates
                .iter()
                .any(|candidate| mentions(candidate, date))
            {
                return Ok(());
            }
            let Some(next) = clickable(&screen.candidates).into_iter().find(|candidate| {
                candidate
                    .name
                    .as_deref()
                    .is_some_and(|name| name.to_lowercase().contains("next month"))
            }) else {
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
            })
            .cloned()
            .collect::<Vec<_>>();
        let purpose = format!("the search box that filters the options of {what}");
        let text = option.to_owned();
        match self.ground(log, screen, &purpose, &purpose, fields).await? {
            Some(grounded) => {
                let app = self.app.clone();
                let target = grounded.candidate;
                let field = target.clone();
                self.act(log, "type to filter", Some(&target), move |backend| {
                    deliver_text(&backend, &app, &field, &text)
                })
                .await?;
                self.history
                    .push(format!("typed into {} to filter it", label(&target)));
            }
            // An opened autocomplete often keeps its input unnamed but
            // focused; typing goes where the focus is.
            None => {
                self.act(log, "type to filter", None, move |backend| {
                    backend.execute(JevOperation::TypeText, None, Some(text))
                })
                .await?;
                self.history
                    .push("typed into the focused field to filter it".to_owned());
            }
        }
        Ok(())
    }

    async fn read(&mut self, log: &mut StepLog, read: &ReadStep) -> Result<Ended, Halt> {
        let what = substitute(&read.what, &self.vars);
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
        let from = substitute(&pick.from, &self.vars);
        let by = substitute(&pick.by, &self.vars);
        let mut screen = self.look().await?;
        self.explore(&mut screen).await;
        let groups = result_groups(&screen);
        if groups.is_empty() {
            return Err(Halt::Failed(format!("no list of {from} is showing")));
        }
        let records = groups
            .iter()
            .map(|group| Record {
                fields: group
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(index, text)| (format!("field {index}"), text.clone()))
                    .collect(),
            })
            .collect::<Vec<_>>();
        let (best, how) =
            match Criterion::parse(&by).and_then(|criterion| rank(&records, criterion)) {
                Some(order) => (order[0], "ranked"),
                None => (
                    self.judge_pick(log, &screen, &from, &by, &groups).await?,
                    "judged",
                ),
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
        let clicked = primary.clone();
        let reply = self
            .act(log, "click", Some(&primary), move |backend| {
                backend.execute(JevOperation::Click, Some(clicked), None)
            })
            .await?;
        if !reply.ok {
            return Err(Halt::Failed(format!(
                "could not open the picked item: {summary}"
            )));
        }
        self.history
            .push(format!("picked {summary} ({how} by {by})"));
        Ok(Ended::new(
            StepOutcome::Done,
            format!("picked {summary} ({how} by {by}, out of {})", groups.len()),
        ))
    }

    /// Stores every item of the list showing as JSON rows of their text.
    async fn extract(&mut self, read: &ReadStep) -> Result<Ended, Halt> {
        let what = substitute(&read.what, &self.vars);
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
        let screen = self.look().await?;
        let pool = clickable(&screen.candidates);
        let Some(grounded) = self
            .ground(log, &screen, &purpose, &purpose, pool)
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
        let condition_text = substitute(&repeat.condition, &self.vars);
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
        let condition_text = substitute(&branch.condition, &self.vars);
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
    "january", "february", "march", "april", "may", "june", "july", "august", "september",
    "october", "november", "december",
];

/// Whether `option` names a calendar day: a month name and a day number.
pub(super) fn looks_like_date(option: &str) -> bool {
    let lower = option.to_lowercase();
    let words = lower
        .split(|character: char| !character.is_alphanumeric())
        .collect::<Vec<_>>();
    words.iter().any(|word| MONTHS.contains(word))
        && words
            .iter()
            .any(|word| word.parse::<u8>().is_ok_and(|day| (1..=31).contains(&day)))
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

/// Whether an element shows `option` in its name, value, or description.
fn mentions(candidate: &Candidate, option: &str) -> bool {
    let option = plain(option);
    !option.is_empty()
        && [
            candidate.name.clone(),
            candidate.description.clone(),
            candidate.value.as_ref().map(ToString::to_string),
        ]
        .into_iter()
        .flatten()
        .any(|text| format!(" {} ", plain(&text)).contains(&format!(" {option} ")))
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
