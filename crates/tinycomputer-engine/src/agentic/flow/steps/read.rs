//! The `read` step: taking one piece of text on screen into a variable.


use serde_json::{Value, json};
use tinycomputer_bus::{
    FlowLoop, ReadStep, StepOutcome,
};


use crate::agentic::flow::{
    Ended, FlowRun, Halt, StepLog,
    ask::{self, Questions, chosen, numbered},
    backend::AgentBackend,
    validate::substitute_safe,
    view::{Candidate, label},
};

use super::LOCATE_FLOOR;

impl<B: AgentBackend + Sync> FlowRun<'_, B> {

    /// Every piece of text a `read` may take, as (label, what Jev is shown,
    /// the text itself): each element's text, its name apart when it says
    /// something the text does not, and the screen's static lines.
    fn read_sources(&self, screen: &crate::agentic::flow::view::Screen) -> Vec<(String, Value, String)> {
        let ordered = ask::ordered_nodes(screen);
        screen
            .candidates
            .iter()
            .flat_map(|candidate| {
                // A rich-text area (a mail body, a web view) holds no value
                // of its own; its text is read from the ref-less nodes
                // `screen.text_nodes` kept for it, the same source
                // `field_contents` reads from for the state Jev already sees.
                let text = ask::rich_text(&ordered, candidate).or_else(|| readable(candidate));
                // An element whose name says something its value does not —
                // a chat's button named for the chat, holding its last
                // message — offers its name as a source of its own, so a
                // read of the name is not handed the value.
                let name = candidate
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty() && text.as_deref() != Some(*name))
                    .map(str::to_owned);
                let shown = |text: &str| {
                    if self.include_values {
                        json!(text)
                    } else {
                        json!(format!("{} characters", text.chars().count()))
                    }
                };
                let value = text.map(|text| {
                    (
                        label(candidate),
                        json!({"untrusted_accessibility_data": {
                            "element": label(candidate),
                            "part": if name.is_some() { "value" } else { "text" },
                            "shows": shown(&text),
                            "state": candidate.states.join(", "),
                        }}),
                        text,
                    )
                });
                // Only beside a value: a name alone is already what the
                // value source shows.
                let name = name.filter(|_| value.is_some()).map(|name| {
                    (
                        label(candidate),
                        json!({"untrusted_accessibility_data": {
                            "element": label(candidate),
                            "part": "name",
                            "shows": name,
                            "state": candidate.states.join(", "),
                        }}),
                        name,
                    )
                });
                name.into_iter().chain(value)
            })
            .chain(screen.context.iter().map(|line| {
                (
                    line.clone(),
                    json!({"untrusted_accessibility_data": {"text": line}}),
                    line.clone(),
                )
            }))
            .collect()
    }

    pub(super) async fn read(&mut self, log: &mut StepLog, read: &ReadStep) -> Result<Ended, Halt> {
        let what = substitute_safe(&read.what, &self.vars, &self.facts);
        let mut screen = self.look().await?;
        self.explore(&mut screen).await;
        let sources = self.read_sources(&screen);
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

}

/// The text an element shows: its value, else its name.
///
/// A control's numeric value (a radio button's `1`) says less than its name,
/// so a named control with a number for a value reads as its name.
pub(in crate::agentic::flow) fn readable(candidate: &Candidate) -> Option<String> {
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
