//! The flow itself and its step kinds, as they are spelled on the wire.

use std::collections::{BTreeMap, BTreeSet};

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, MapAccess, Visitor},
    ser::SerializeMap,
};
use serde_json::Value;

use crate::{JevMetrics, JevTarget};

/// A high-level, app-agnostic script of what to accomplish in one application.
///
/// A flow never names a UI element, a shortcut, or a menu. Each step says
/// *what* should happen ("start a new email message"); the module grounds it
/// on the live screen with Jev decision loops. That is what lets a caller that
/// has never seen the application write one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flow {
    /// The application the flow drives, by name as the platform knows it.
    pub app: String,
    /// Named values steps can reference as `${name}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, String>,
    /// The steps, run in order.
    pub steps: Vec<FlowStep>,
}

/// One step of a [`Flow`].
///
/// On the wire a step is either a bare string, which is a [`FlowAction::Do`]
/// intent, or an object with exactly one key naming the step kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowStep {
    /// A bare intent string: reach the described state.
    Intent(String),
    /// A structured step.
    Action(FlowAction),
}

impl FlowStep {
    /// The action this step performs, with a bare intent read as `do`.
    #[must_use]
    pub fn action(&self) -> FlowAction {
        match self {
            Self::Intent(intent) => FlowAction::Do(intent.clone()),
            Self::Action(action) => action.clone(),
        }
    }
}

impl Serialize for FlowStep {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Intent(intent) => serializer.serialize_str(intent),
            Self::Action(action) => action.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for FlowStep {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StepVisitor)
    }
}

/// Reads a step straight from the input rather than through a
/// `serde_json::Value`, so a parsed `enter` step keeps its document order and a
/// malformed step gets an error naming its kind.
struct StepVisitor;

impl<'de> Visitor<'de> for StepVisitor {
    type Value = FlowStep;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a step: a string, or an object with exactly one key naming its kind")
    }

    fn visit_str<E: de::Error>(self, intent: &str) -> Result<FlowStep, E> {
        Ok(FlowStep::Intent(intent.to_owned()))
    }

    fn visit_string<E: de::Error>(self, intent: String) -> Result<FlowStep, E> {
        Ok(FlowStep::Intent(intent))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<FlowStep, A::Error> {
        let Some(kind) = access.next_key::<String>()? else {
            return Err(de::Error::custom(
                "a step object must have exactly one key naming its kind, found none",
            ));
        };
        let context = |error: A::Error| de::Error::custom(format!("in `{kind}` step: {error}"));
        let action = match kind.as_str() {
            "open" => FlowAction::Open(access.next_value().map_err(context)?),
            "browse" => FlowAction::Browse(access.next_value().map_err(context)?),
            "do" => FlowAction::Do(access.next_value().map_err(context)?),
            "enter" => FlowAction::Enter(access.next_value().map_err(context)?),
            "choose" => FlowAction::Choose(access.next_value().map_err(context)?),
            "read" => FlowAction::Read(access.next_value().map_err(context)?),
            "pick" => FlowAction::Pick(access.next_value().map_err(context)?),
            "extract" => FlowAction::Extract(access.next_value().map_err(context)?),
            "verify" => FlowAction::Verify(access.next_value().map_err(context)?),
            "wait_for" => FlowAction::WaitFor(access.next_value().map_err(context)?),
            "stop_before" => FlowAction::StopBefore(access.next_value().map_err(context)?),
            "repeat_until" => FlowAction::RepeatUntil(access.next_value().map_err(context)?),
            "if" => FlowAction::If(access.next_value().map_err(context)?),
            _ => {
                return Err(de::Error::custom(format!(
                    "unknown step kind `{kind}`; expected a string or one of {}",
                    STEP_KINDS.join(", ")
                )));
            }
        };
        if let Some(extra) = access.next_key::<String>()? {
            return Err(de::Error::custom(format!(
                "a step object must have exactly one key naming its kind, found `{kind}` and `{extra}`"
            )));
        }
        Ok(FlowStep::Action(action))
    }
}

/// Every structured step kind, as spelled on the wire.
pub const STEP_KINDS: &[&str] = &[
    "open",
    "browse",
    "do",
    "enter",
    "choose",
    "read",
    "pick",
    "extract",
    "verify",
    "wait_for",
    "stop_before",
    "repeat_until",
    "if",
];

/// A structured flow step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowAction {
    /// Launch the named application, or bring it forward.
    Open(String),
    /// Open a web address in the browser, and continue the flow there until
    /// an `open` step switches back to an application.
    Browse(String),
    /// Reach the described state ("the Liked Songs list is open").
    Do(String),
    /// Put each text into the thing its slot describes.
    Enter(Slots),
    /// Pick an option in a list, menu, or popup.
    Choose(ChooseStep),
    /// Capture the visible text or value of the described thing.
    Read(ReadStep),
    /// Choose the best of a list of results by a criterion, and open it.
    Pick(PickStep),
    /// Capture every item of a list of results into a variable.
    Extract(ReadStep),
    /// Require a natural-language condition to hold; the flow fails if not.
    Verify(String),
    /// Wait until a natural-language condition holds.
    WaitFor(String),
    /// Locate an irreversible action and stop in front of it unless the run
    /// allows destructive steps.
    StopBefore(String),
    /// Repeat steps until a condition holds, at most `max` times.
    RepeatUntil(RepeatStep),
    /// Run one branch or the other depending on a condition.
    If(IfStep),
}

/// One text and the slot it goes into, for [`FlowAction::Enter`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    /// What the text is for, in plain words ("recipient", "message body").
    pub slot: String,
    /// The text to enter. `${name}` references a flow variable.
    pub text: String,
}

/// The slots of an [`FlowAction::Enter`] step.
///
/// On the wire this is a JSON object from slot to text. Document order is kept
/// when parsing text, but key order does not survive a trip through a
/// `serde_json::Value`, which is how `TinyBus` carries arguments. So the module
/// never relies on it: it fills the matched fields top to bottom as they
/// appear on screen, which is the order a form's tab order and autocomplete
/// expect anyway.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Slots(pub Vec<Slot>);

impl Serialize for Slots {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for slot in &self.0 {
            map.serialize_entry(&slot.slot, &slot.text)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Slots {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SlotsVisitor;

        impl<'de> Visitor<'de> for SlotsVisitor {
            type Value = Slots;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an object from slot description to text")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Slots, A::Error> {
                let mut slots = Vec::new();
                while let Some((slot, text)) = access.next_entry::<String, String>()? {
                    slots.push(Slot { slot, text });
                }
                Ok(Slots(slots))
            }
        }

        deserializer.deserialize_map(SlotsVisitor)
    }
}

/// Payload of [`FlowAction::Choose`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseStep {
    /// The list, menu, or popup to choose in.
    pub what: String,
    /// The option to pick, in the words it is shown with.
    pub option: String,
}

/// Payload of [`FlowAction::Read`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadStep {
    /// The thing whose text to capture ("the subject of the newest message").
    pub what: String,
    /// The variable to store it in.
    pub into: String,
}

/// Payload of [`FlowAction::Pick`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickStep {
    /// The list to pick from ("the flight results").
    pub from: String,
    /// What makes one the best: "lowest price", "earliest departure", or any
    /// plain description. Prices, times, durations, and stops are compared
    /// exactly; anything else is judged.
    pub by: String,
    /// The variable to store the picked item's text in, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub into: Option<String>,
}

/// Payload of [`FlowAction::RepeatUntil`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatStep {
    /// The condition that ends the repetition.
    pub condition: String,
    /// The steps to repeat.
    pub steps: Vec<FlowStep>,
    /// Most repetitions. Absent means 5; capped by the module at 20.
    #[serde(default = "default_repeat")]
    pub max: u32,
}

fn default_repeat() -> u32 {
    5
}

/// Payload of [`FlowAction::If`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IfStep {
    /// The condition to judge on the current screen.
    pub condition: String,
    /// Steps run when the condition holds.
    #[serde(default)]
    pub then: Vec<FlowStep>,
    /// Steps run when it does not.
    #[serde(default, rename = "else")]
    pub otherwise: Vec<FlowStep>,
}
