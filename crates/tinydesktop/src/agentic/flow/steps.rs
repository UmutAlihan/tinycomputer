//! One function per step kind; `run` dispatches between them.

use serde_json::{Value, json};
use tinydesktop_bus::{
    ChooseStep, FlowAction, FlowLoop, FlowStopReason, IfStep, JevOperation, ReadStep, RepeatStep,
    StepOutcome,
};

use super::{
    AgentBackend, Ended, FlowRun, Halt, StepLog,
    act::DONE,
    ask::{self, Questions, chosen, condition, numbered},
    memory::{learn, remember},
    validate::{MAX_REPEAT, substitute},
    view::{Candidate, is_destructive, label, target_payload},
};

/// Turns a `do` step may spend.
const DO_TURNS: u32 = 8;
/// Turns spent opening the thing a `choose` step picks from.
const REVEAL_TURNS: u32 = 3;
/// Times `open` checks for a readable window, waiting between checks.
const WINDOW_CHECKS: u32 = 10;
/// Times a `wait_for` checks its condition, waiting between checks.
const WAIT_CHECKS: u32 = 10;
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
        FlowAction::Do(_) => run.accomplish(log, text, DO_TURNS).await,
        FlowAction::Enter(slots) => run.enter(log, &slots.0).await,
        FlowAction::Choose(choose) => run.choose(log, choose).await,
        FlowAction::Read(read) => run.read(log, read).await,
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
        let purpose = format!("pick the option {option:?} in {what}");
        for attempt in 0..2 {
            let screen = self.look().await?;
            let pool = clickable(&screen.candidates)
                .into_iter()
                .filter(|candidate| !is_destructive(candidate, &screen, &self.stop_before))
                .collect::<Vec<_>>();
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
                    self.history
                        .push(format!("chose {option:?} with {}", label(&target)));
                    return Ok(Ended::new(StepOutcome::Done, format!("chose {option:?}")));
                }
            }
            if attempt == 0 {
                self.accomplish(
                    log,
                    &format!("open {what} so its options show"),
                    REVEAL_TURNS,
                )
                .await?;
            }
        }
        Err(Halt::Failed(format!("{option:?} was not found in {what}")))
    }

    async fn read(&mut self, log: &mut StepLog, read: &ReadStep) -> Result<Ended, Halt> {
        let what = substitute(&read.what, &self.vars);
        let mut screen = self.look().await?;
        self.explore(&mut screen).await;
        let mut sources: Vec<(String, Value, String)> = screen
            .candidates
            .iter()
            .filter_map(|candidate| {
                let text = readable(candidate)?;
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
            .collect();
        sources.extend(screen.context.iter().map(|line| {
            (
                line.clone(),
                json!({"untrusted_accessibility_data": {"text": line}}),
                line.clone(),
            )
        }));
        if sources.is_empty() {
            return Err(Halt::Failed(format!("nothing readable shows {what}")));
        }
        sources.truncate(ask::MAX_READ_SOURCES);
        let keys = numbered(sources.len());
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
                                sources
                                    .iter()
                                    .map(|(_, description, _)| description.clone()),
                            ),
                        ),
                    ),
                ),
            )
            .await?;
        let Some((choice, confidence)) =
            chosen(&answers, "source").filter(|(_, confidence)| *confidence >= LOCATE_FLOOR)
        else {
            return Err(Halt::Failed(format!(
                "no text on screen clearly shows {what}"
            )));
        };
        let Some((source, _, text)) = keys
            .iter()
            .position(|key| *key == choice)
            .and_then(|index| sources.get(index))
        else {
            return Err(Halt::Failed(format!(
                "no text on screen clearly shows {what}"
            )));
        };
        log.confidence = Some(confidence);
        self.vars.insert(read.into.clone(), text.clone());
        self.history
            .push(format!("read {what} from {source} into {}", read.into));
        Ok(Ended::new(
            StepOutcome::Done,
            format!(
                "read {} characters into {}",
                text.chars().count(),
                read.into
            ),
        ))
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
            self.step.clone_from(path);
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
