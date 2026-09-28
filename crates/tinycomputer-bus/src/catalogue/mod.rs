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

mod agentic;
mod browser;
mod desktop;
mod types;

pub use types::{Family, Member, MemberSummary};

use agentic::{FLOW, TASK};
use browser::BROWSER;
use desktop::DESKTOP;

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

/// Every served member, in [`crate::names::METHODS`] order: the flow and task
/// members, then the desktop and browser primitives.
pub const MEMBERS: &[Member] = &ALL;

/// How many members the module serves.
const LEN: usize = FLOW.len() + TASK.len() + DESKTOP.len() + BROWSER.len();

/// The families' entries laid end to end, built once at compile time.
const ALL: [Member; LEN] = concat([FLOW, TASK, DESKTOP, BROWSER]);

/// Lays `parts` end to end. A `const` slice cannot be concatenated by the
/// language, so this copies entry by entry into an array of the known length.
const fn concat(parts: [&[Member]; 4]) -> [Member; LEN] {
    let mut all = [parts[0][0]; LEN];
    let mut at = 0;
    let mut part = 0;
    while part < parts.len() {
        let mut index = 0;
        while index < parts[part].len() {
            all[at] = parts[part][index];
            at += 1;
            index += 1;
        }
        part += 1;
    }
    all
}

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
mod catalogue_tests;
