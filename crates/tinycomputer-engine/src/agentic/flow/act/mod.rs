//! The loop behind a `do` step: judge, choose a move, act, judge again.
//!
//! Every turn asks one request about the current screen: whether the step is
//! already accomplished (completion), how far along it is (progress), whether
//! something unrelated is in the way (obstacles), and which app-agnostic move
//! to make next (moves). Only an `activate`, `expand`, or `scroll` move needs
//! an element, and that is grounded separately with the narrowing loops.
//!
//! The moves are generic on purpose: a flow names no UI, so the next move is
//! picked from things every application offers — pressing a visible control,
//! a standard shortcut, scrolling, waiting.
//!
//! A deliberating run (`docs/technical/specs/jev-deliberation.md`) adds a loop around
//! every press. Before it, the press's effect is predicted (`expect/`) and
//! a checkpoint taken (`checkpoint/`); after it, the effect is checked, and
//! when the screen contradicts it the next judgement asks whether the press
//! did what it was meant to. A mistake is undone back to the checkpoint —
//! verified — and the next-best
//! candidate grounding ranked is tried before grounding again. A judgement
//! of "done" near its threshold is settled on its evidence (`escalate`), and
//! a screen that returns to where it was two turns ago bans both presses.
//!
//! The loop's pieces: `turns` runs it, `judge` reads each turn's screen,
//! `moves` makes the chosen move, and `recover` undoes a turn that went
//! wrong. This root holds the thresholds and the state they share.

mod judge;
mod moves;
mod recover;
mod turns;

pub(super) use judge::Judgement;

use std::{collections::BTreeSet, time::Instant};

use tinycomputer_bus::StepOutcome;

use super::{
    Ended, StepLog,
    attention::Cleared,
    checkpoint::Checkpoint,
    expect::{Effect, Outcome},
    view::{Candidate, Screen},
};

/// Completion probability that ends a step after acting.
pub(super) const DONE: f64 = 0.75;
/// Completion probability that skips a step before acting.
const ALREADY_DONE: f64 = 0.85;
/// Completion probability under which the judge leans "not done".
const LEANS_DONE: f64 = 0.5;
/// Obstacle probability that triggers dismissal.
const BLOCKED: f64 = 0.7;
/// A progress drop, as a fraction of the scale, that counts as a regression.
const REGRESSION: f64 = 0.25;
/// Probability that the last action helped below which it is undone.
const UNHELPFUL: f64 = 0.2;
/// Least probability a shortcut choice needs to be pressed.
const SHORTCUT_FLOOR: f64 = 0.5;
/// Unchanged turns after which a step gives up.
const STALL_TURNS: u32 = 3;
/// Waits in a row that changed nothing after which Jev is not let wait again.
const MAX_IDLE_WAITS: u32 = 2;
/// Obstacles dismissed per step at most.
const MAX_OBSTACLES: u32 = 2;
/// Undos per step at most.
const MAX_UNDOS: u32 = 2;
/// Belief that a press did what it was meant to under which a press whose
/// effect the screen contradicts is taken for a mistake.
pub(super) const MISTAKE: f64 = 0.5;
/// Belief under which any press is taken for a mistake, whatever the
/// screen shows of its effect.
pub(super) const CLEAR_MISTAKE: f64 = 0.25;
/// Next-best candidates a `do` step backtracks into at most, deep and
/// standard.
pub(super) const MAX_BRANCHES: (u32, u32) = (3, 1);
/// The view of the screen alone, without the history that can lead a
/// judgement.
pub(super) const SCREEN_VIEW: &str =
    "Judge only from the screen as it is shown now; no history of actions is given.";
/// The view of what changed since the step began.
const CHANGES_VIEW: &str =
    "Judge from what changed on screen since the step began, and the actions taken.";

/// Generic moves every application offers, with what each is for.
const MOVES: &[(&str, &str)] = &[
    (
        "activate",
        "Press one visible control: a button, link, tab, list row, toolbar item, or menu item.",
    ),
    (
        "shortcut",
        "Use a standard keyboard shortcut, such as creating a new item or opening search.",
    ),
    (
        "expand",
        "Open a collapsed section, disclosure, or dropdown.",
    ),
    ("scroll", "Scroll a list or page to reveal more of it."),
    (
        "wait",
        "The application is visibly still loading; wait for it.",
    ),
    (
        "finished",
        "The step is already accomplished; nothing more is needed.",
    ),
    (
        "stuck",
        "Nothing on screen, and no standard shortcut, can move toward the step.",
    ),
];

/// Standard macOS shortcuts that are safe to try: none sends, deletes, or
/// quits. `confirm` (Return) commits text typed into a field; it is refused
/// while a sheet or alert is showing, where Return presses the default button.
pub(super) const SHORTCUTS: &[(&str, &str, &str)] = &[
    (
        "new_item",
        "cmd+n",
        "Create a new item: a new document, message, note, window, or event.",
    ),
    ("new_folder", "cmd+shift+n", "Create a new folder."),
    ("find", "cmd+f", "Search or find within the application."),
    ("reply", "cmd+r", "Reply to the selected message."),
    ("settings", "cmd+,", "Open the application's settings."),
    ("back", "cmd+[", "Go back to the previous view."),
    ("next_field", "tab", "Move focus to the next field."),
    (
        "confirm",
        "return",
        "Commit the text just typed into a field, such as a new name.",
    ),
    (
        "dismiss",
        "escape",
        "Close a popup, menu, or dialog without acting.",
    ),
];

/// What the last action was, so a regression can be undone sensibly.
#[derive(Debug, Clone)]
struct LastAction {
    target: Option<Candidate>,
    before: Screen,
    progress: Option<f64>,
    /// Whether the action was a wait rather than a press or a shortcut.
    waited: bool,
    /// Under deliberation: what the press should have changed, and where it
    /// started.
    expected: Option<Expected>,
    /// How the screen after the press bears out `expected`.
    outcome: Option<Outcome>,
}

/// What a deliberating press expects, and the checkpoint it can be undone
/// back to.
#[derive(Debug, Clone)]
pub(super) struct Expected {
    effect: Effect,
    checkpoint: Checkpoint,
}

/// Bookkeeping across the turns of one `do` step.
#[derive(Debug, Default)]
struct DoState {
    /// The turn under way, when it began, and how many decisions and round
    /// trips the run had made by then: the journal's `turn` event.
    turn: Option<(u32, Instant, u32, u32)>,
    last: Option<LastAction>,
    banned: BTreeSet<String>,
    unchanged: u32,
    /// Waits in a row that changed nothing.
    idle_waits: u32,
    obstacles: u32,
    undos: u32,
    /// Under deliberation: each turn's screen fingerprint, oldest first, and
    /// the element pressed on the turn before the last, for oscillations.
    seen: Vec<String>,
    pressed_before: Option<Candidate>,
    /// The screen the step began on, for the judge's changes view.
    first: Option<Screen>,
    /// Next-best candidates tried after mistakes so far.
    branches: u32,
    /// The candidate a backtrack tries next.
    branch: Option<Candidate>,
    /// Distractions cleared this step (`attention/`).
    cleared: Cleared,
}

/// What a move did.
#[derive(Debug)]
enum Move {
    /// The step is over.
    Ended(Ended),
    /// An action ran, on this element if it had one, expecting this.
    Acted(Option<Box<Candidate>>, Option<Box<Expected>>),
    /// Nothing ran this turn.
    Skipped,
}

/// Whether `intent` asks for something new to be made ("start a new email",
/// "create a folder").
///
/// Such a step can never be accomplished before acting: a draft or folder that
/// is already on screen is someone else's, and treating it as the new one is
/// how a flow ends up writing into a person's own unsent draft.
pub(super) fn creates_new(intent: &str) -> bool {
    let words = intent
        .split(|character: char| !character.is_alphanumeric())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    words
        .iter()
        .any(|word| matches!(word.as_str(), "new" | "create"))
}

/// Words of a step that ask for an overlay to go away.
const DISMISS_VERBS: &[&str] = &["dismiss", "close", "accept", "decline", "reject", "skip"];

/// What such a step asks to go away.
const OVERLAYS: &[&str] = &[
    "banner", "dialog", "popup", "cookie", "cookies", "consent", "modal", "overlay", "prompt",
    "notice",
];

fn words(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Ends a step the screen itself shows done: the last action pressed a
/// control on an overlay — one the step names ("accepting essential only"
/// for "Accept Essential Only"), or any control when the step is about
/// dismissing that overlay — and the overlay has closed.
///
/// A completion judge sees only the screen after the fact, where a closed
/// cookie banner leaves no trace of which button closed it; this is the
/// evidence it cannot see.
fn closed_the_overlay(last: &LastAction, screen: &Screen, intent: &str) -> Option<Ended> {
    let name = last.target.as_ref()?.name.as_deref()?;
    if last.before.surface == "window" || screen.surface != "window" {
        return None;
    }
    let intent = words(intent);
    let named = words(name)
        .iter()
        .filter(|word| word.len() > 2)
        .all(|word| intent.iter().any(|said| said.starts_with(word.as_str())))
        && words(name).iter().any(|word| word.len() > 2);
    let dismissal = intent
        .iter()
        .any(|word| DISMISS_VERBS.contains(&word.as_str()))
        && intent.iter().any(|word| OVERLAYS.contains(&word.as_str()));
    (named || dismissal).then(|| {
        Ended::new(
            StepOutcome::Done,
            format!("pressed {name:?} and the {} closed", last.before.surface),
        )
    })
}

/// Whether an action was refused because something covers its target.
fn covered(reply: &tinycomputer_bus::DesktopResponse) -> bool {
    reply
        .error
        .as_ref()
        .is_some_and(|error| error.message.contains("is covered by"))
}

/// The completion a step needs on `turn`: more before acting, since
/// skipping a step that was not done derails everything after it.
fn threshold(turn: u32) -> f64 {
    if turn == 0 { ALREADY_DONE } else { DONE }
}

/// The completion under which Jev's "finished" move is overruled on `turn`.
fn finish_floor(turn: u32) -> f64 {
    if turn == 0 { ALREADY_DONE } else { LEANS_DONE }
}

/// Ends the step when the completion judge is confident enough.
fn finished(log: &mut StepLog, judged: &Judgement, turn: u32) -> Option<Ended> {
    let done = judged.done?;
    log.confidence = Some(done);
    let outcome = if turn == 0 {
        StepOutcome::AlreadyDone
    } else {
        StepOutcome::Done
    };
    (done >= threshold(turn))
        .then(|| Ended::new(outcome, format!("accomplished (confidence {done:.2})")))
}

/// What grounding looks for when a move of `verb` serves `intent`.
fn activate_purpose(verb: &str, intent: &str) -> String {
    format!("{verb} to accomplish: {intent}")
}
