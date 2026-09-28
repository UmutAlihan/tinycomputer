//! The wide strategy: one request per turn, over a digest of the screen and
//! the run's working memory.
//!
//! The narrow strategy asks one small question after another about the same
//! screen: judge it, then choose an element, then re-ask when unsure, then
//! narrow a crowded screen region by region. Jev is cheap and its window is
//! large, so the wide strategy asks everything a turn might need at once:
//!
//! - the same judging questions (`done`, `not_done`, `progress`, `blocked`,
//!   `helped`, `move`, `shortcut`);
//! - `dismiss`, over the controls of whatever is in front, so an obstacle
//!   is cleared without another round trip, and `dismiss_known` when an
//!   earlier run remembered which control closed it;
//! - for each move that needs an element — `activate`, `expand`, `scroll` —
//!   a `target_*` Choice over the candidates the attention pass ranks
//!   highest, its reversed and relabelled `again_*` for consistency, a
//!   `known_*` confirmation of a remembered element, and, for a pool larger
//!   than one Choice, a knockout of `group_*` Choices instead.
//!
//! The runtime then applies the narrow strategy's thresholds to answers it
//! already holds. A second request is made only when the chosen target is
//! not confident (one `confirm` Noul) or a knockout left several winners
//! (one final Choice). A move whose target Jev answered `none` is not
//! re-asked that turn: the next turn looks again.
//!
//! Every wide question is asked against a state that shows the screen as a
//! [digest](tinycomputer_core::surface::digest) — what is in front, regions
//! ranked by relevance, lists as cards, noise collapsed — and a memory
//! section built by the [ledger](super::ledger) in place of the flat history.

mod judge;
mod resolve;
mod state;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use tinycomputer_bus::{FlowLoop, FlowStrategy, JevOperation};
use tinyinference_decisions::Answer;

use super::{
    AgentBackend, FlowRun, Halt, StepLog,
    act::Judgement,
    ask::{self, CAP, Questions, chosen, corroborate, elements, lettered, numbered, probability},
    ground::{AGREED, CORROBORATED, Grounded, NAMED_FLOOR},
    ledger::Context,
    memory::{learn, recall, remember},
    view::{
        ACT, Candidate, Digest, Rendering, Screen, digest, distinct, element_kind,
        exact_named_match, is_destructive, label, named_first, signature,
    },
};

/// Bytes of screen a wide request shows before regions are collapsed.
/// Dense page text runs near 2.7 bytes a token, so this is about nine
/// thousand tokens: at 40,000 a live booking page reached 28,000 tokens of
/// Jev's 32,000 once the questions were added.
pub(super) const DIGEST_BUDGET: usize = 24_000;

/// How many of the run's saved variables every state recalls: the most
/// recent ones, enough for a step that walks a list to know which items
/// it has done.
pub(super) const MAX_COLLECTED: usize = 12;

/// How much of each saved value the state recalls.
pub(super) const COLLECTED_CHARS: usize = 120;
/// Most candidates one move is offered: two Choices of [`CAP`]. The survey
/// ranks the relevant regions first, and a larger knockout mostly added a
/// final round.
const WIDE_POOL: usize = CAP * 2;
/// The moves that need an element, with the action it must support and the
/// verb its purpose is phrased with.
const TARGETED: [(&str, &str, &str); 3] = [
    ("activate", "Click", "click"),
    ("expand", "Expand", "expand"),
    ("scroll", "Scroll", "scroll"),
];
/// What dismissing an obstacle is for, as a confirmation question puts it.
const DISMISS_PURPOSE: &str = "close what is in front without losing work";

/// A target chosen for a move before the move was made.
#[derive(Debug, Clone)]
pub(super) enum Prepared {
    /// Confident enough to use as it is.
    Chosen(Grounded),
    /// Chosen, but not confidently: it is used only if a `confirm` Noul
    /// agrees, the way the narrow strategy corroborates a hesitant pick.
    Unsure {
        candidate: Candidate,
        confidence: f64,
        /// Whether the consistency re-ask was asked, and whether it agreed.
        consistency: Option<bool>,
    },
    /// A knockout's group winners, still to be chosen among.
    Finals(Vec<Candidate>),
    /// Asked, and nothing fits: the move is not re-asked this turn.
    Nothing,
}

/// How to clear what is in front.
#[derive(Debug, Clone)]
pub(super) struct Dismissal {
    /// The region in front, by name, for grounding memory.
    pub(super) front: String,
    /// The control to press, or `None` to press Escape.
    pub(super) control: Option<Candidate>,
}

/// One move's candidate targets, as asked.
struct TargetPlan {
    operation: &'static str,
    purpose: String,
    known: Option<Candidate>,
    groups: Vec<Vec<Candidate>>,
}

/// The grounding-memory key an obstacle's dismissal is remembered under.
fn obstacle_key(front: &str) -> String {
    format!("obstacle: {front}")
}

/// Moves the element pressed last turn to the end of `pool`: pressing a
/// toggle again closes what it just opened (measured on a booking widget,
/// where the destination button, named by the step, led the options and was
/// pressed twice). An element that changed nothing is banned instead.
pub(super) fn pressed_last(pool: &mut Vec<Candidate>, pressed: Option<&Candidate>) {
    if let Some(pressed) = pressed
        && let Some(at) = pool
            .iter()
            .position(|candidate| same_element(candidate, pressed))
    {
        let again = pool.remove(at);
        pool.push(again);
    }
}

/// Whether two snapshots' candidates are the same element: role and place,
/// and a label one extends the other by — a dropdown button's name often
/// grows by the list it opened ("destinationCity Empty" becomes
/// "destinationCity Empty POPULAR DESTINATIONS …") — ignoring the value
/// and states pressing it changes.
fn same_element(left: &Candidate, right: &Candidate) -> bool {
    let name = |candidate: &Candidate| {
        candidate
            .name
            .clone()
            .or_else(|| candidate.description.clone())
            .unwrap_or_default()
    };
    let (left_name, right_name) = (name(left), name(right));
    let (short, long) = if left_name.len() <= right_name.len() {
        (&left_name, &right_name)
    } else {
        (&right_name, &left_name)
    };
    left.role == right.role
        && left.path == right.path
        && (short == long || (short.chars().count() >= 3 && long.starts_with(short.as_str())))
}

/// Whether `candidate` supports the engine action `capability`.
fn supports(candidate: &Candidate, capability: &str) -> bool {
    candidate
        .available_actions
        .iter()
        .any(|action| action == capability)
}

/// The element of `pool` a numbered Choice `id` picked, with its
/// probability.
fn pick(
    answers: &BTreeMap<String, Answer>,
    id: &str,
    pool: &[Candidate],
) -> Option<(Candidate, f64)> {
    let (key, probability) = chosen(answers, id)?;
    let index = key.parse::<usize>().ok()?.checked_sub(1)?;
    pool.get(index)
        .cloned()
        .map(|candidate| (candidate, probability))
}

/// How the answers say to clear what is in front: a remembered control Jev
/// confirmed, the control it chose, or Escape; `None` when it chose nothing.
fn dismissal(
    answers: &BTreeMap<String, Answer>,
    front: String,
    known: Option<Candidate>,
    pool: &[Candidate],
) -> Option<Dismissal> {
    if let Some(known) = known
        && probability(answers, "dismiss_known").is_some_and(|confirmed| confirmed >= AGREED)
    {
        return Some(Dismissal {
            front,
            control: Some(known),
        });
    }
    match chosen(answers, "dismiss") {
        Some((key, _)) if key == "escape" => Some(Dismissal {
            front,
            control: None,
        }),
        Some(_) => pick(answers, "dismiss", pool).map(|(control, _)| Dismissal {
            front,
            control: Some(control),
        }),
        None => None,
    }
}
