//! The evidence behind an answer, and the verdict it supports.
//!
//! Jev's probabilities measure how concentrated an answer is, not how likely
//! it is to be right: a Choice at 0.72 may be a clear pick among many weak
//! options or a coin toss between two lookalikes. A single threshold on that
//! number decides both the same way. Deliberation reads the *ballot* instead
//! — every framing's own answer (`vote.rs`) — and asks three things:
//!
//! - **how high** the winner stands (`p`),
//! - **how far ahead** of the runner-up (`margin`),
//! - **how many framings** picked it (`agreement`).
//!
//! From those a [`Verdict`] follows: act on it ([`Verdict::Accept`]), ask
//! more before acting ([`Verdict::Deliberate`]), or leave it
//! ([`Verdict::Abstain`]). Everything here is pure: the flow runtime asks,
//! this module only reads. `docs/specs/jev-deliberation.md` is the contract,
//! and every constant is listed in `docs/decision-thresholds.md`.

use tinyinference_decisions::Answer;

/// Least lead the winner of a Choice needs over the runner-up to be acted on
/// without asking more.
pub(super) const ACCEPT_MARGIN: f64 = 0.25;
/// Least share of framings that must have picked the winner to act on it
/// without asking more.
pub(super) const ACCEPT_AGREEMENT: f64 = 0.8;
/// A winner this weak, picked by fewer framings than [`ABSTAIN_AGREEMENT`],
/// is left rather than deliberated: nothing on screen serves.
pub(super) const ABSTAIN_FLOOR: f64 = 0.2;
/// Share of framings under which a weak winner is abstained from.
pub(super) const ABSTAIN_AGREEMENT: f64 = 0.4;
/// Half-width of the band around a yes/no threshold inside which a
/// judgement is deliberated rather than taken at face value.
pub(super) const UNDECIDED_BAND: f64 = 0.12;

/// What a ballot shows about one answer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Evidence {
    /// The winner's mean probability; for a yes/no, the mean belief.
    pub(super) p: f64,
    /// The winner's lead over the runner-up; for a yes/no, the belief's
    /// distance from its threshold, doubled so a sure answer reads 1.
    pub(super) margin: f64,
    /// The share of framings that agreed with the merged answer.
    pub(super) agreement: f64,
    /// How far the framings' own values spread: their standard deviation.
    pub(super) spread: f64,
    /// How many framings the ballot holds.
    pub(super) framings: usize,
    /// Whether the merged winner is `none`: nothing offered serves.
    pub(super) none: bool,
}

/// What to do with an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verdict {
    /// The evidence is strong enough to act on.
    Accept,
    /// The evidence is thin: ask more before acting.
    Deliberate,
    /// The evidence says nothing offered serves.
    Abstain,
}

impl Verdict {
    /// The verdict's wire spelling, for the journal.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Deliberate => "deliberate",
            Self::Abstain => "abstain",
        }
    }
}

/// The bar a Choice's winner must clear at one decision site.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Bar {
    /// Least mean probability.
    pub(super) floor: f64,
    /// Least lead over the runner-up.
    pub(super) margin: f64,
    /// Least share of framings agreeing.
    pub(super) agreement: f64,
}

impl Bar {
    /// A bar with the default margin and agreement over `floor`.
    pub(super) const fn over(floor: f64) -> Self {
        Self {
            floor,
            margin: ACCEPT_MARGIN,
            agreement: ACCEPT_AGREEMENT,
        }
    }
}

/// The evidence a Choice ballot gives for `merged`'s winner, or `None` when
/// `merged` is not a Choice.
///
/// A ballot of one framing agrees with itself: its agreement is 1, and only
/// its margin can call for deliberation.
pub(super) fn of_choice(merged: &Answer, ballot: &[Answer]) -> Option<Evidence> {
    let Answer::Choice(merged) = merged else {
        return None;
    };
    let p = merged
        .probabilities
        .get(&merged.choice)
        .copied()
        .unwrap_or_default();
    let runner_up = merged
        .probabilities
        .iter()
        .filter(|(key, _)| **key != merged.choice)
        .map(|(_, probability)| *probability)
        .fold(0.0_f64, f64::max);
    let picks = ballot
        .iter()
        .filter_map(|answer| match answer {
            Answer::Choice(choice) => Some(choice),
            _ => None,
        })
        .collect::<Vec<_>>();
    let agreeing = picks
        .iter()
        .filter(|choice| choice.choice == merged.choice)
        .count();
    let values = picks
        .iter()
        .map(|choice| {
            choice
                .probabilities
                .get(&merged.choice)
                .copied()
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    Some(Evidence {
        p,
        margin: (p - runner_up).max(0.0),
        agreement: share(agreeing, picks.len()),
        spread: deviation(&values),
        framings: picks.len(),
        none: merged.choice == "none",
    })
}

/// The evidence a series of per-framing beliefs gives about whether they
/// clear `threshold`: `p` is their mean, `agreement` the share on the same
/// side of `threshold` as the mean.
pub(super) fn of_beliefs(beliefs: &[f64], threshold: f64) -> Evidence {
    let p = mean(beliefs);
    let above = p >= threshold;
    let agreeing = beliefs
        .iter()
        .filter(|belief| (**belief >= threshold) == above)
        .count();
    let reach = if above { 1.0 - threshold } else { threshold };
    Evidence {
        p,
        margin: if reach > 0.0 {
            ((p - threshold).abs() / reach).min(1.0)
        } else {
            1.0
        },
        agreement: share(agreeing, beliefs.len()),
        spread: deviation(beliefs),
        framings: beliefs.len(),
        none: false,
    }
}

/// What a Choice's evidence supports against `bar`.
pub(super) fn choice_verdict(evidence: &Evidence, bar: &Bar) -> Verdict {
    if evidence.none {
        return Verdict::Abstain;
    }
    if evidence.p >= bar.floor
        && evidence.margin >= bar.margin
        && evidence.agreement >= bar.agreement
    {
        return Verdict::Accept;
    }
    if evidence.p < ABSTAIN_FLOOR && evidence.agreement < ABSTAIN_AGREEMENT {
        return Verdict::Abstain;
    }
    Verdict::Deliberate
}

/// What a yes/no judgement's evidence supports against `threshold`:
/// accepted as it reads when it lies outside [`UNDECIDED_BAND`] of the
/// threshold and its framings agree, deliberated otherwise. A judgement is
/// never abstained from: it always has an answer, yes or no.
pub(super) fn belief_verdict(evidence: &Evidence, threshold: f64) -> Verdict {
    let clear = (evidence.p - threshold).abs() >= UNDECIDED_BAND;
    if clear && evidence.agreement >= ACCEPT_AGREEMENT {
        Verdict::Accept
    } else {
        Verdict::Deliberate
    }
}

/// `part` of `whole` as a fraction; 1 for an empty whole, which nothing
/// contradicts.
fn share(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        return 1.0;
    }
    count(part) / count(whole)
}

fn count(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

/// The mean of `values`; 0 for none.
pub(super) fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / count(values.len())
}

/// The median of `values`: the middle one, or the mean of the middle two;
/// 0 for none.
pub(super) fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    match sorted.len() {
        0 => 0.0,
        length if length % 2 == 1 => sorted[middle],
        _ => f64::midpoint(sorted[middle - 1], sorted[middle]),
    }
}

/// The population standard deviation of `values`; 0 for fewer than two.
fn deviation(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = mean(values);
    (values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / count(values.len()))
    .sqrt()
}

#[cfg(test)]
mod test;
