//! Wire types for high-level intent flows.

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

/// One Jev decision loop.
///
/// Reported per step in [`StepReport::loops`], and named in
/// [`RunFlowRequest::disabled_loops`] to measure what a loop contributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowLoop {
    /// The completion judge.
    Completion,
    /// The progress judge.
    Progress,
    /// The app-agnostic move chooser.
    Moves,
    /// Region-by-region narrowing.
    Narrowing,
    /// Yes/no corroboration of a target.
    Corroboration,
    /// Relabelled re-asking.
    Consistency,
    /// Slot-to-field matching for `enter`.
    Slots,
    /// Obstacle detection and dismissal.
    Obstacles,
    /// Undo and try the next candidate.
    Undo,
    /// Grounding memory.
    Memory,
}

/// Where an element was found for one step, so a later run can try it first.
///
/// It carries no ref: refs die with their snapshot. Role, label, and ancestor
/// path are what survive between runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(default)]
pub struct GroundingHint {
    /// Application the element lives in.
    pub app: String,
    /// The normalized step text or slot the element grounded.
    pub key: String,
    /// Accessibility role.
    pub role: String,
    /// Accessible name, when it has one.
    pub name: Option<String>,
    /// Labels of the element's ancestors, outermost first.
    pub path: Vec<String>,
}

/// Runs a [`Flow`] with Jev decision loops.
///
/// Requires confidential delivery, like `RunGoal`: the texts a flow enters
/// travel with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RunFlowRequest {
    /// The flow to run.
    pub flow: Flow,
    /// Values for `${name}` references, overriding the flow's own `vars`.
    pub vars: BTreeMap<String, String>,
    /// Names among `vars` whose value is a caller-supplied fact rather than
    /// an ordinary flow variable.
    ///
    /// A fact may be typed only where a step delivers text locally: an
    /// `enter` step's value, or substituted into a `browse` address or an
    /// `open` application name. Anywhere else `${name}` may appear in a
    /// flow — a `do`, `verify`, `wait_for`, or `stop_before` text, a
    /// `choose`'s `what`/`option`, a `read`/`extract`'s `what`, a `pick`'s
    /// `from`/`by`, a `repeat_until`/`if` condition, or an `enter` slot's
    /// name — never sees a fact's value, because that text is what Jev is
    /// asked to reason about. [`crate::FlowValidation`] rejects a flow that
    /// references a fact there, and the flow runtime never expands one even
    /// if that check were bypassed.
    pub facts: BTreeSet<String>,
    /// Whether `stop_before` steps may perform their irreversible action.
    pub allow_destructive: bool,
    /// Whether ordinary field values may leave the machine for Jev.
    pub include_values: bool,
    /// Most desktop actions for the whole run; capped by the module at 120.
    pub max_actions: u32,
    /// Most Jev evaluations for the whole run; capped by the module at 300.
    pub max_model_calls: u32,
    /// Decision loops to turn off. Empty in production; set to measure what
    /// one loop contributes. [`FlowLoop::Slots`] cannot be turned off.
    pub disabled_loops: Vec<FlowLoop>,
    /// Grounding hints from earlier runs.
    pub memory: Vec<GroundingHint>,
    /// Whether to return every Jev exchange in [`FlowRunResult::trace`]. For
    /// development: the trace carries the screen state each question saw.
    pub trace: bool,
}

impl Default for RunFlowRequest {
    fn default() -> Self {
        Self {
            flow: Flow::default(),
            vars: BTreeMap::new(),
            allow_destructive: false,
            include_values: false,
            max_actions: 60,
            max_model_calls: 150,
            disabled_loops: Vec::new(),
            memory: Vec::new(),
            trace: false,
        }
    }
}

/// Checks a flow without touching the desktop or Jev.
///
/// The flow is taken as raw JSON so a malformed one comes back as a list of
/// readable errors rather than as a bus decode failure — which is what a
/// model writing flows needs in order to repair one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ValidateFlowRequest {
    /// The candidate flow.
    pub flow: Value,
}

/// Result of [`ValidateFlowRequest`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowValidation {
    /// Whether the flow can be run.
    pub valid: bool,
    /// Every problem found, each naming the step it is in.
    pub errors: Vec<String>,
    /// Steps counted, including nested ones.
    pub steps: usize,
}

/// Why a flow run stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowStopReason {
    /// Every step finished.
    Completed,
    /// A `stop_before` step found its irreversible action and stopped in
    /// front of it. Every earlier step finished.
    StoppedBeforeDestructive,
    /// A step could not be accomplished.
    StepFailed,
    /// The action budget ran out.
    ActionBudget,
    /// The Jev call budget ran out.
    ModelBudget,
    /// The flow did not validate.
    Invalid,
}

/// How one step ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepOutcome {
    /// The step was accomplished by acting.
    Done,
    /// The completion judge found the step already accomplished.
    AlreadyDone,
    /// The step's irreversible action was found and not performed.
    Gated,
    /// The step could not be accomplished.
    Failed,
}

/// One desktop action a step took.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowActionRecord {
    /// What was done: `click`, `fill`, `press cmd+n`, `undo`, `launch`, ….
    pub action: String,
    /// The element acted on, when there was one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<JevTarget>,
    /// Whether the desktop reported success.
    pub ok: bool,
    /// Detail: the delivery path, the error code, what changed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// What one step did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepReport {
    /// Position in the flow: `3`, or `4.2` for the second step nested in the
    /// fourth.
    pub path: String,
    /// The step kind as spelled on the wire.
    pub kind: String,
    /// The step's text, with variables substituted.
    pub text: String,
    /// How it ended.
    pub outcome: StepOutcome,
    /// Decision turns spent.
    pub turns: u32,
    /// Jev evaluations spent.
    pub jev_calls: u32,
    /// Desktop actions taken.
    pub actions: Vec<FlowActionRecord>,
    /// Decision loops that contributed.
    pub loops: Vec<FlowLoop>,
    /// Final completion or target confidence, when one was measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Why it ended the way it did.
    pub note: String,
}

/// Result of [`RunFlowRequest`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowRunResult {
    /// Why the run stopped.
    pub stop: FlowStopReason,
    /// One report per step reached, in order, nested steps included.
    pub steps: Vec<StepReport>,
    /// Variables at the end of the run, including those `read` steps set.
    pub vars: BTreeMap<String, String>,
    /// The irreversible action a `stop_before` step stopped in front of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<JevTarget>,
    /// Grounding hints learned this run, for the caller to pass back later.
    pub learned: Vec<GroundingHint>,
    /// Desktop actions taken.
    pub actions: u32,
    /// Provider measurements.
    pub metrics: JevMetrics,
    /// Every Jev exchange, when [`RunFlowRequest::trace`] asked for them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<JevExchange>,
}

/// One Jev request and its answers, as recorded by a traced run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JevExchange {
    /// The step that asked, as in [`StepReport::path`].
    pub step: String,
    /// The shared state the questions were asked against.
    pub state: Value,
    /// The questions, keyed by id.
    pub questions: Value,
    /// The answers, keyed by id.
    pub answers: Value,
}
