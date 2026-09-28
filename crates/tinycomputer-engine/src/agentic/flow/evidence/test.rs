//! Tests for reading a ballot into evidence and a verdict.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use tinyinference_decisions::{Answer, ChoiceAnswer, NoulAnswer};

use super::{
    ABSTAIN_FLOOR, Bar, UNDECIDED_BAND, Verdict, belief_verdict, choice_verdict, mean, of_beliefs,
    of_choice,
};

fn choice(winner: &str, probabilities: &[(&str, f64)]) -> Answer {
    Answer::Choice(ChoiceAnswer {
        choice: winner.to_owned(),
        probabilities: probabilities
            .iter()
            .map(|(key, probability)| ((*key).to_owned(), *probability))
            .collect::<BTreeMap<_, _>>(),
        confidence: 0.0,
    })
}

#[test]
fn a_clear_unanimous_pick_is_accepted() {
    let pick = choice("1", &[("1", 0.9), ("2", 0.05), ("none", 0.05)]);
    let evidence = of_choice(&pick, &[pick.clone(), pick.clone(), pick.clone()]).unwrap();
    assert!((evidence.margin - 0.85).abs() < 1e-9);
    assert!((evidence.agreement - 1.0).abs() < 1e-9);
    assert_eq!(evidence.framings, 3);
    assert_eq!(choice_verdict(&evidence, &Bar::over(0.7)), Verdict::Accept);
}

#[test]
fn a_high_pick_with_a_thin_margin_is_deliberated() {
    // Two lookalikes: the winner stands high only because the options are
    // few, and the runner-up is right behind it.
    let pick = choice("1", &[("1", 0.55), ("2", 0.45), ("none", 0.0)]);
    let evidence = of_choice(&pick, std::slice::from_ref(&pick)).unwrap();
    assert!(evidence.margin < 0.25);
    assert_eq!(
        choice_verdict(&evidence, &Bar::over(0.5)),
        Verdict::Deliberate
    );
}

#[test]
fn a_split_vote_is_deliberated_even_when_the_mean_is_high() {
    let first = choice("1", &[("1", 0.95), ("2", 0.05)]);
    let second = choice("2", &[("1", 0.4), ("2", 0.6)]);
    let merged = choice("1", &[("1", 0.8), ("2", 0.2)]);
    let evidence = of_choice(
        &merged,
        &[first.clone(), first.clone(), first, second.clone(), second],
    )
    .unwrap();
    assert!((evidence.agreement - 0.6).abs() < 1e-9);
    assert!(evidence.spread > 0.1);
    assert_eq!(
        choice_verdict(&evidence, &Bar::over(0.7)),
        Verdict::Deliberate
    );
}

#[test]
fn none_winning_is_an_abstention() {
    let pick = choice("none", &[("1", 0.1), ("none", 0.9)]);
    let evidence = of_choice(&pick, std::slice::from_ref(&pick)).unwrap();
    assert!(evidence.none);
    assert_eq!(choice_verdict(&evidence, &Bar::over(0.1)), Verdict::Abstain);
}

#[test]
fn a_weak_scattered_pick_is_an_abstention() {
    let a = choice("1", &[("1", 0.15), ("2", 0.1), ("3", 0.1)]);
    let b = choice("2", &[("1", 0.1), ("2", 0.15), ("3", 0.1)]);
    let c = choice("3", &[("1", 0.1), ("2", 0.1), ("3", 0.15)]);
    let merged = choice("1", &[("1", 0.12), ("2", 0.117), ("3", 0.116)]);
    let evidence = of_choice(&merged, &[a, b, c]).unwrap();
    assert!(evidence.p < ABSTAIN_FLOOR);
    assert_eq!(choice_verdict(&evidence, &Bar::over(0.7)), Verdict::Abstain);
}

#[test]
fn only_a_choice_has_choice_evidence() {
    let noul = Answer::Noul(NoulAnswer { noul: 0.9 });
    assert!(of_choice(&noul, &[]).is_none());
}

#[test]
fn a_belief_near_its_threshold_is_deliberated() {
    // The live audit's case: a wide judge at 0.78 against the 0.75 bar.
    let evidence = of_beliefs(&[0.78], 0.75);
    assert!((evidence.p - 0.78).abs() < 1e-9);
    assert_eq!(belief_verdict(&evidence, 0.75), Verdict::Deliberate);
}

#[test]
fn a_clear_agreed_belief_is_accepted_either_way() {
    assert_eq!(
        belief_verdict(&of_beliefs(&[0.95, 0.9, 0.97], 0.75), 0.75),
        Verdict::Accept
    );
    assert_eq!(
        belief_verdict(&of_beliefs(&[0.05, 0.1], 0.75), 0.75),
        Verdict::Accept
    );
}

#[test]
fn framings_that_straddle_the_threshold_are_deliberated() {
    let evidence = of_beliefs(&[0.99, 0.99, 0.99, 0.5, 0.5], 0.75);
    assert!(evidence.p >= 0.75 && evidence.p < 0.75 + UNDECIDED_BAND);
    assert!((evidence.agreement - 0.6).abs() < 1e-9);
    assert_eq!(belief_verdict(&evidence, 0.75), Verdict::Deliberate);
}

#[test]
fn belief_margin_is_scaled_to_the_room_on_its_side() {
    assert!((of_beliefs(&[1.0], 0.75).margin - 1.0).abs() < 1e-9);
    assert!((of_beliefs(&[0.0], 0.75).margin - 1.0).abs() < 1e-9);
    assert!((of_beliefs(&[0.875], 0.75).margin - 0.5).abs() < 1e-9);
    assert!((of_beliefs(&[1.0], 1.0).margin - 1.0).abs() < 1e-9);
}

#[test]
fn an_empty_ballot_reads_as_unanimous_and_the_mean_of_nothing_is_zero() {
    let evidence = of_beliefs(&[], 0.5);
    assert!((evidence.agreement - 1.0).abs() < 1e-9);
    assert!(mean(&[]).abs() < 1e-9);
}

#[test]
fn verdicts_have_wire_names() {
    assert_eq!(Verdict::Accept.name(), "accept");
    assert_eq!(Verdict::Deliberate.name(), "deliberate");
    assert_eq!(Verdict::Abstain.name(), "abstain");
}
