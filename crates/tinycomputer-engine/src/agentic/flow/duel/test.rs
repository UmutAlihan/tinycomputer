//! Tests for pairwise duels: the questions and the Copeland count.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use tinyinference_decisions::{Answer, ChoiceAnswer, Question};

use super::{DUEL_WIN, finalists, id, questions, standing};
use crate::agentic::flow::view::Candidate;

fn button(name: &str) -> Candidate {
    Candidate {
        ref_id: format!("@{name}"),
        role: "button".to_owned(),
        name: Some(name.to_owned()),
        available_actions: vec!["Click".to_owned()],
        ..Candidate::default()
    }
}

fn duel(first: f64, second: f64) -> Answer {
    Answer::Choice(ChoiceAnswer {
        choice: if first >= second { "1" } else { "2" }.to_owned(),
        probabilities: [
            ("1".to_owned(), first),
            ("2".to_owned(), second),
            ("none".to_owned(), 0.0),
        ]
        .into_iter()
        .collect(),
        confidence: 1.0,
    })
}

#[test]
fn every_pairing_is_asked_in_both_orders() {
    let finalists = [button("A"), button("B"), button("C")];
    let asked = questions("press to pay", &finalists, false);
    assert_eq!(asked.len(), 6);
    let (_, Question::Choice(first)) = &asked[0] else {
        panic!("a duel is a choice")
    };
    assert_eq!(asked[0].0, id(0, 1));
    assert!(
        first.criteria["1"]
            .as_ref()
            .unwrap()
            .to_string()
            .contains('A')
    );
    assert!(
        first.criteria["2"]
            .as_ref()
            .unwrap()
            .to_string()
            .contains('B')
    );
    assert!(first.criteria.contains_key("none"));
    assert!(asked.iter().any(|(question, _)| *question == id(1, 0)));
}

#[test]
fn a_candidate_that_beats_every_rival_is_champion() {
    // B beats A and C in both orders; the order shown never decides.
    let answers = BTreeMap::from([
        (id(0, 1), duel(0.2, 0.8)),
        (id(1, 0), duel(0.8, 0.2)),
        (id(0, 2), duel(0.6, 0.4)),
        (id(2, 0), duel(0.4, 0.6)),
        (id(1, 2), duel(0.9, 0.1)),
        (id(2, 1), duel(0.1, 0.9)),
    ]);
    let standing = standing(&answers, 3);
    assert_eq!(standing.champion, Some(1));
    assert_eq!(standing.order[0], 1);
    assert_eq!(standing.wins, vec![1, 2, 0]);
}

#[test]
fn position_bias_cancels_across_both_orders() {
    // Each order prefers whichever is shown first: the pairing is even and
    // neither is champion.
    let answers = BTreeMap::from([(id(0, 1), duel(0.8, 0.2)), (id(1, 0), duel(0.8, 0.2))]);
    let standing = standing(&answers, 2);
    assert_eq!(standing.champion, None);
    assert_eq!(standing.wins, vec![0, 0]);
    assert!((standing.strength[0] - 0.5).abs() < 1e-9);
}

#[test]
fn a_narrow_win_is_not_a_championship() {
    let edge = f64::midpoint(0.5, DUEL_WIN);
    let answers = BTreeMap::from([
        (id(0, 1), duel(edge, 1.0 - edge)),
        (id(1, 0), duel(1.0 - edge, edge)),
    ]);
    let standing = standing(&answers, 2);
    assert_eq!(standing.wins, vec![1, 0]);
    assert_eq!(standing.champion, None);
    assert_eq!(standing.order, vec![0, 1]);
}

#[test]
fn unanswered_pairings_are_even_and_one_finalist_is_no_duel() {
    let standing2 = standing(&BTreeMap::new(), 2);
    assert_eq!(standing2.champion, None);
    assert_eq!(standing(&BTreeMap::new(), 1).champion, None);
    let refused = BTreeMap::from([(id(0, 1), duel(0.0, 0.0))]);
    assert_eq!(standing(&refused, 2).wins, vec![0, 0]);
}

#[test]
fn finalists_are_the_strongest_real_options() {
    let answer = Answer::Choice(ChoiceAnswer {
        choice: "3".to_owned(),
        probabilities: [
            ("1", 0.3),
            ("2", 0.01),
            ("3", 0.35),
            ("4", 0.1),
            ("5", 0.08),
            ("6", 0.06),
            ("none", 0.1),
        ]
        .into_iter()
        .map(|(key, probability)| (key.to_owned(), probability))
        .collect(),
        confidence: 0.4,
    });
    assert_eq!(finalists(&answer), vec!["3", "1", "4", "5"]);
    assert!(
        finalists(&Answer::Noul(tinyinference_decisions::NoulAnswer {
            noul: 0.5
        }))
        .is_empty()
    );
}
