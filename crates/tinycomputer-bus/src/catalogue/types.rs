//! The catalogue's types: a member's family, its static entry, and the form
//! `Describe` serves it in.

use serde::{Deserialize, Serialize};

/// Which part of the module a member belongs to, and so which reply shape and
/// which level of abstraction it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    /// The task members: hand over a goal, get back progress, questions,
    /// checkpoints, and results. No refs, no selectors. Replies are an
    /// [`crate::agent::AgentResponse`]. Start here.
    Task,
    /// One-shot Jev members below the task API: run a flow, a goal, or an
    /// intent directly, or check a flow. Replies are a
    /// [`crate::DesktopResponse`].
    Flow,
    /// Desktop primitives over the accessibility tree: snapshot, act on a
    /// ref, manage apps and windows. Replies are a [`crate::DesktopResponse`].
    Desktop,
    /// Browser primitives over a session: navigate, snapshot, perform, read,
    /// screenshot. Replies are a [`crate::DesktopResponse`].
    Browser,
}

/// One served member, as the contract catalogues it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Member {
    /// The member name, as a host calls it.
    pub name: &'static str,
    /// The family it belongs to.
    pub family: Family,
    /// What it does, in one sentence.
    pub summary: &'static str,
    /// Whether frames to it must be delivered confidentially.
    pub confidential: bool,
}

/// One served member, as `Describe` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberSummary {
    /// The member name, as a host calls it.
    pub name: String,
    /// The family it belongs to.
    pub family: Family,
    /// What it does, in one sentence.
    pub summary: String,
    /// Whether frames to it must be delivered confidentially.
    pub confidential: bool,
}

impl From<&Member> for MemberSummary {
    fn from(member: &Member) -> Self {
        Self {
            name: member.name.to_owned(),
            family: member.family,
            summary: member.summary.to_owned(),
            confidential: member.confidential,
        }
    }
}
