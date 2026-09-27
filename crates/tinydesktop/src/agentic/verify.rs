//! Deterministic checks of the caller's accessibility-visible success conditions.

use tinydesktop_bus::{JevObservation, JevPredicateResult, VisiblePredicate};

use super::screen::{Candidate, Screen};

pub(super) fn exact_label(candidate: &Candidate, label: &str) -> bool {
    candidate
        .labels()
        .any(|candidate_label| candidate_label.eq_ignore_ascii_case(label))
}

pub(super) fn verify(screen: &Screen, predicates: &[VisiblePredicate]) -> JevObservation {
    JevObservation {
        app: screen.app.clone(),
        window: screen.window.clone(),
        surface: screen.surface.clone(),
        predicates: predicates
            .iter()
            .map(|predicate| {
                verify_one(
                    if screen.observed.is_empty() {
                        &screen.candidates
                    } else {
                        &screen.observed
                    },
                    predicate,
                )
            })
            .collect(),
    }
}

pub(super) fn satisfied(observation: &JevObservation) -> bool {
    !observation.predicates.is_empty() && observation.predicates.iter().all(|item| item.matched)
}

fn verify_one(candidates: &[Candidate], predicate: &VisiblePredicate) -> JevPredicateResult {
    if let VisiblePredicate::NameContains { fragment, within } = predicate {
        let ancestor = format!(" {within:?}");
        let matched_candidate = candidates.iter().find(|candidate| {
            candidate
                .name
                .as_deref()
                .is_some_and(|name| name.contains(fragment))
                && candidate.path.iter().any(|part| part.ends_with(&ancestor))
        });
        return JevPredicateResult {
            predicate: predicate.clone(),
            matched: matched_candidate.is_some(),
            observed_name: matched_candidate.and_then(|candidate| candidate.name.clone()),
            observed_value: None,
            observed_states: Vec::new(),
        };
    }
    let name = match predicate {
        VisiblePredicate::NamePresent { name }
        | VisiblePredicate::ValueEquals { name, .. }
        | VisiblePredicate::ValueContains { name, .. }
        | VisiblePredicate::StateContains { name, .. } => name,
        VisiblePredicate::NameContains { .. } => unreachable!("handled above"),
    };
    let candidates_with_name = candidates
        .iter()
        .filter(|candidate| exact_label(candidate, name))
        .collect::<Vec<_>>();
    let unique = (candidates_with_name.len() == 1).then(|| candidates_with_name[0]);
    let matched = match predicate {
        VisiblePredicate::NamePresent { .. } => !candidates_with_name.is_empty(),
        VisiblePredicate::ValueEquals { value, .. } => unique.is_some_and(|candidate| {
            candidate.value.as_ref().and_then(serde_json::Value::as_str) == Some(value.as_str())
        }),
        VisiblePredicate::ValueContains { value, .. } => unique.is_some_and(|candidate| {
            candidate
                .value
                .as_ref()
                .and_then(serde_json::Value::as_str)
                .is_some_and(|observed| observed.contains(value))
        }),
        VisiblePredicate::StateContains { state, .. } => unique.is_some_and(|candidate| {
            candidate
                .states
                .iter()
                .any(|observed| observed.eq_ignore_ascii_case(state))
        }),
        VisiblePredicate::NameContains { .. } => unreachable!("handled above"),
    };
    JevPredicateResult {
        predicate: predicate.clone(),
        matched,
        observed_name: unique.map(|_| name.clone()),
        observed_value: if matched {
            match predicate {
                VisiblePredicate::ValueEquals { value, .. }
                | VisiblePredicate::ValueContains { value, .. } => Some(value.clone()),
                _ => None,
            }
        } else {
            None
        },
        observed_states: if matches!(predicate, VisiblePredicate::StateContains { .. }) {
            if matched {
                if let VisiblePredicate::StateContains { state, .. } = predicate {
                    vec![state.clone()]
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        },
    }
}

#[cfg(test)]
mod test;
