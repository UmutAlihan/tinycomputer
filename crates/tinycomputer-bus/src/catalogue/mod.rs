//! The catalogue: every served member, with its family, a one-line summary,
//! and whether it is confidential — the map a model reads before it picks a
//! member.
//!
//! [`crate::names::METHODS`] says *what* is served; this says what each one is
//! *for*. `Describe` serves it as `Capabilities.catalogue`, so an agent that
//! has only ever called `Describe` knows every member exists and which family
//! to reach for: the task members first, the flow members for a one-shot Jev
//! run, and the desktop and browser primitives when it wants to drive the
//! screen itself. The entries are in [`crate::names::METHODS`] order, and a
//! unit test holds the two together.

mod types;

pub use types::{Family, Member, MemberSummary};

use crate::browser::names::methods as browser;
use crate::names::methods;

const fn entry(
    name: &'static str,
    family: Family,
    summary: &'static str,
    confidential: bool,
) -> Member {
    Member {
        name,
        family,
        summary,
        confidential,
    }
}

/// Every served member, in [`crate::names::METHODS`] order.
pub const MEMBERS: &[Member] = &[
    entry(methods::RESOLVE_INTENT, Family::Flow, "Resolves one natural-language intent against the current screen.", true),
    entry(methods::RUN_GOAL, Family::Flow, "Runs a bounded Jev observe-decide-act loop toward a goal on the desktop.", true),
    entry(methods::RUN_FLOW, Family::Flow, "Runs a high-level intent flow on the desktop and returns per-step reports.", true),
    entry(methods::VALIDATE_FLOW, Family::Flow, "Checks a flow without touching the desktop or Jev.", false),
    entry(methods::FLOW_GUIDE, Family::Flow, "Returns the flow authoring guide as prompt text.", false),
    entry(methods::DESCRIBE, Family::Task, "Everything a caller needs: surfaces, the guide, schemas, examples, and this catalogue.", false),
    entry(methods::PLAN_TASK, Family::Task, "Drafts a flow for a plain-language task without acting; needs a planner.", false),
    entry(methods::START_TASK, Family::Task, "Starts a task from a flow or a plain-language goal and returns at once.", true),
    entry(methods::AWAIT_TASK, Family::Task, "Waits until a task needs something, finishes, or the timeout passes.", false),
    entry(methods::CONTINUE_TASK, Family::Task, "Answers a paused task — inputs, an approval, an answer — and resumes it.", true),
    entry(methods::CANCEL_TASK, Family::Task, "Stops a task and releases its browser session.", false),
    entry(methods::TASK_REPORT, Family::Task, "Everything a task did: steps, records, rescues, learned hints, and trace.", true),
    entry(methods::LIST_TASKS, Family::Task, "The tasks this module holds, newest first.", false),
    entry(methods::SNAPSHOT, Family::Desktop, "Walks an application's accessibility tree and allocates a ref per element.", false),
    entry(methods::FIND, Family::Desktop, "Returns the elements matching a role, name, or text query.", false),
    entry(methods::GET, Family::Desktop, "Reads one property of one ref.", false),
    entry(methods::IS, Family::Desktop, "Tests one boolean state of one ref.", false),
    entry(methods::SCREENSHOT, Family::Desktop, "Captures an application, window, or display as an image.", false),
    entry(methods::CLICK, Family::Desktop, "Clicks a ref through its accessibility action.", false),
    entry(methods::DOUBLE_CLICK, Family::Desktop, "Double-clicks a ref.", false),
    entry(methods::TRIPLE_CLICK, Family::Desktop, "Triple-clicks a ref.", false),
    entry(methods::RIGHT_CLICK, Family::Desktop, "Right-clicks a ref, opening its context menu.", false),
    entry(methods::TYPE, Family::Desktop, "Types text into a ref.", false),
    entry(methods::SET_VALUE, Family::Desktop, "Replaces a ref's value in one step.", false),
    entry(methods::CLEAR, Family::Desktop, "Empties a ref's value.", false),
    entry(methods::FOCUS, Family::Desktop, "Gives a ref keyboard focus.", false),
    entry(methods::SELECT, Family::Desktop, "Selects an option within a ref.", false),
    entry(methods::TOGGLE, Family::Desktop, "Toggles a checkable ref.", false),
    entry(methods::CHECK, Family::Desktop, "Checks a checkable ref.", false),
    entry(methods::UNCHECK, Family::Desktop, "Unchecks a checkable ref.", false),
    entry(methods::EXPAND, Family::Desktop, "Expands an expandable ref.", false),
    entry(methods::COLLAPSE, Family::Desktop, "Collapses an expandable ref.", false),
    entry(methods::SCROLL, Family::Desktop, "Scrolls a scrollable ref.", false),
    entry(methods::SCROLL_TO, Family::Desktop, "Scrolls a ref into view.", false),
    entry(methods::PRESS, Family::Desktop, "Presses a key combination.", false),
    entry(methods::KEY_DOWN, Family::Desktop, "Reserved for a stateful daemon; fails closed.", false),
    entry(methods::KEY_UP, Family::Desktop, "Reserved for a stateful daemon; fails closed.", false),
    entry(methods::HOVER, Family::Desktop, "Moves the cursor over a ref or a point.", false),
    entry(methods::DRAG, Family::Desktop, "Drags between two endpoints.", false),
    entry(methods::MOUSE_MOVE, Family::Desktop, "Moves the cursor to a point.", false),
    entry(methods::MOUSE_CLICK, Family::Desktop, "Clicks at a point.", false),
    entry(methods::MOUSE_DOWN, Family::Desktop, "Reserved for a stateful daemon; fails closed.", false),
    entry(methods::MOUSE_UP, Family::Desktop, "Reserved for a stateful daemon; fails closed.", false),
    entry(methods::MOUSE_WHEEL, Family::Desktop, "Scrolls the wheel at a point.", false),
    entry(methods::LAUNCH, Family::Desktop, "Starts or attaches to an application.", false),
    entry(methods::CLOSE_APP, Family::Desktop, "Quits or terminates an application.", false),
    entry(methods::LIST_APPS, Family::Desktop, "Lists running applications.", false),
    entry(methods::LIST_WINDOWS, Family::Desktop, "Lists windows.", false),
    entry(methods::LIST_DISPLAYS, Family::Desktop, "Lists displays.", false),
    entry(methods::LIST_SURFACES, Family::Desktop, "Lists the surfaces an application currently exposes.", false),
    entry(methods::FOCUS_WINDOW, Family::Desktop, "Brings a window forward.", false),
    entry(methods::RESIZE_WINDOW, Family::Desktop, "Resizes a window.", false),
    entry(methods::MOVE_WINDOW, Family::Desktop, "Moves a window.", false),
    entry(methods::MINIMIZE, Family::Desktop, "Minimizes a window.", false),
    entry(methods::MAXIMIZE, Family::Desktop, "Maximizes a window.", false),
    entry(methods::RESTORE, Family::Desktop, "Restores a minimized or maximized window.", false),
    entry(methods::CLIPBOARD_GET, Family::Desktop, "Reads the pasteboard.", false),
    entry(methods::CLIPBOARD_SET, Family::Desktop, "Writes the pasteboard.", false),
    entry(methods::CLIPBOARD_CLEAR, Family::Desktop, "Empties the pasteboard.", false),
    entry(methods::LIST_NOTIFICATIONS, Family::Desktop, "Lists notification-centre entries.", false),
    entry(methods::NOTIFICATION_ACTION, Family::Desktop, "Invokes an action on a notification.", false),
    entry(methods::DISMISS_NOTIFICATION, Family::Desktop, "Dismisses one notification.", false),
    entry(methods::DISMISS_ALL_NOTIFICATIONS, Family::Desktop, "Dismisses every notification, optionally for one application.", false),
    entry(methods::WAIT, Family::Desktop, "Blocks until a condition holds or the timeout expires.", false),
    entry(methods::VERSION, Family::Desktop, "Reports the engine version and target.", false),
    entry(methods::STATUS, Family::Desktop, "Reports permissions, the active session, and the latest snapshot.", false),
    entry(methods::PERMISSIONS, Family::Desktop, "Reports, and optionally prompts for, the permissions automation needs.", false),
    entry(browser::OPEN_SESSION, Family::Browser, "Launches or attaches a browser and returns its session.", false),
    entry(browser::CLOSE_SESSION, Family::Browser, "Closes a session and everything it owns; closing a gone one succeeds.", false),
    entry(browser::LIST_SESSIONS, Family::Browser, "Lists the browser sessions this module holds, a task's included.", false),
    entry(browser::NAVIGATE, Family::Browser, "Navigates a session's active page and reports where it settled.", false),
    entry(browser::SNAPSHOT, Family::Browser, "Captures the active page's accessibility tree with element refs.", false),
    entry(browser::PERFORM, Family::Browser, "Performs one interaction — click, fill, press, scroll, wait — on the active page.", false),
    entry(browser::READ_PAGE, Family::Browser, "Extracts the active page as text or markdown.", false),
    entry(browser::EVALUATE, Family::Browser, "Evaluates JavaScript in the active page and returns its value.", false),
    entry(browser::SCREENSHOT, Family::Browser, "Captures the page and holds the image for BrowserReadOutput.", false),
    entry(browser::READ_OUTPUT, Family::Browser, "Reads one base64 chunk of a held screenshot or PDF.", false),
    entry(browser::RELEASE_OUTPUT, Family::Browser, "Releases a held output before it expires.", false),
    entry(browser::LIST_DOWNLOADS, Family::Browser, "Lists a session's retained downloads.", false),
    entry(browser::WAIT_DOWNLOAD, Family::Browser, "Waits for a session's next download to finish.", false),
];

/// The catalogue entry for `name`, if the module serves it.
///
/// # Examples
///
/// ```
/// # use tinycomputer_bus::catalogue::{self, Family};
/// let entry = catalogue::member("StartTask").expect("StartTask is served");
/// assert_eq!(entry.family, Family::Task);
/// assert!(entry.confidential);
/// assert!(catalogue::member("Teleport").is_none());
/// ```
#[must_use]
pub fn member(name: &str) -> Option<&'static Member> {
    MEMBERS.iter().find(|member| member.name == name)
}

/// The catalogue in the form `Describe` serves it.
#[must_use]
pub fn summaries() -> Vec<MemberSummary> {
    MEMBERS.iter().map(MemberSummary::from).collect()
}

#[cfg(test)]
mod test;
