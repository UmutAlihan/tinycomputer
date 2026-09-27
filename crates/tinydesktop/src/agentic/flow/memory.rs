//! Grounding memory: which element accomplished which step before.
//!
//! A hint is stored by role, name, and ancestor labels, never by ref, so it
//! survives across snapshots and across runs. The caller keeps hints between
//! runs by passing [`tinydesktop_bus::FlowRunResult::learned`] back in as
//! [`tinydesktop_bus::RunFlowRequest::memory`]; the module holds no files.

use tinydesktop_bus::GroundingHint;

use super::{super::screen::Candidate, validate::normalize};

/// Ancestor labels compared when matching a hint; deeper ones are more stable.
const PATH_TAIL: usize = 2;

/// The hint for `key` in `app`, as stored.
pub(super) fn remember(app: &str, key: &str, candidate: &Candidate) -> GroundingHint {
    GroundingHint {
        app: app.to_owned(),
        key: normalize(key),
        role: candidate.role.clone(),
        name: candidate
            .name
            .clone()
            .or_else(|| candidate.description.clone()),
        path: candidate.path.clone(),
    }
}

/// The candidate on screen that a remembered hint for `key` points at.
pub(super) fn recall<'a>(
    hints: &[GroundingHint],
    app: &str,
    key: &str,
    pool: &'a [Candidate],
) -> Option<&'a Candidate> {
    let key = normalize(key);
    hints
        .iter()
        .rev()
        .filter(|hint| hint.app == app && hint.key == key)
        .find_map(|hint| pool.iter().find(|candidate| matches(hint, candidate)))
}

fn matches(hint: &GroundingHint, candidate: &Candidate) -> bool {
    let name = candidate
        .name
        .as_ref()
        .or(candidate.description.as_ref());
    hint.role == candidate.role
        && hint.name.as_ref() == name
        && tail(&hint.path) == tail(&candidate.path)
}

fn tail(path: &[String]) -> &[String] {
    &path[path.len().saturating_sub(PATH_TAIL)..]
}

/// Adds `hint`, replacing an older one for the same app and key.
pub(super) fn learn(hints: &mut Vec<GroundingHint>, hint: GroundingHint) {
    hints.retain(|existing| !(existing.app == hint.app && existing.key == hint.key));
    hints.push(hint);
}
