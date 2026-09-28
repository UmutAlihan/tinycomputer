//! The pure helpers: substitution, the guide, answer readers, region
//! splitting, and grounding memory.

use super::*;

#[test]
fn text_helpers_substitute_reference_and_normalize() {
    let vars = BTreeMap::from([("to".to_owned(), "sam".to_owned())]);
    assert_eq!(
        validate::substitute("hi ${to}, ${other}", &vars),
        "hi sam, ${other}"
    );
    assert_eq!(
        validate::references("${a} and ${b} and ${unclosed"),
        ["a", "b"]
    );
    assert_eq!(
        validate::normalize("  The Message-Body! "),
        "the message body"
    );
    assert_eq!(validate::step_path("", 0), "1");
    assert_eq!(validate::step_path("4", 1), "4.2");
    assert_eq!(
        validate::substitute(
            "${a}",
            &BTreeMap::from([
                ("a".to_owned(), "${b}".to_owned()),
                ("b".to_owned(), "leaked".to_owned()),
            ]),
        ),
        "${b}",
        "a substituted value must not be rescanned for further references"
    );
    assert_eq!(
        validate::substitute("trailing ${unclosed", &BTreeMap::new()),
        "trailing ${unclosed"
    );
}

#[test]
fn substitute_safe_never_expands_a_fact_even_if_asked_to() {
    let vars = BTreeMap::from([
        ("email".to_owned(), "sam@example.com".to_owned()),
        ("topic".to_owned(), "budget".to_owned()),
    ]);
    let facts = BTreeSet::from(["email".to_owned()]);
    assert_eq!(
        validate::substitute_safe("send to ${email} about ${topic}", &vars, &facts),
        "send to ${email} about budget"
    );
    assert_eq!(
        validate::substitute_safe("${email}", &vars, &BTreeSet::new()),
        "sam@example.com",
        "a non-fact name still substitutes normally"
    );
}

#[test]
fn guide_and_answer_helpers_behave() {
    let guide = flow_guide();
    assert!(
        guide.data.unwrap()["guide"]
            .as_str()
            .unwrap()
            .contains("# Writing a desktop flow")
    );
    assert_eq!(ask::lettered(28)[..3], ["A", "B", "C"]);
    assert_eq!(ask::lettered(28)[26..], ["AA", "AB"]);
    let answers = BTreeMap::from([
        ("progress".to_owned(), level(4)),
        (
            "flat".to_owned(),
            Answer::Score(ScoreAnswer {
                score: 0.0,
                legend: BTreeMap::new(),
                probabilities: BTreeMap::from([("0".to_owned(), 1.0)]),
                confidence: 1.0,
            }),
        ),
        ("yes".to_owned(), noul(0.7)),
    ]);
    assert!((ask::level(&answers, "progress").unwrap() - 1.0).abs() < 1e-9);
    assert!(ask::level(&answers, "flat").is_none());
    assert!(ask::level(&answers, "yes").is_none());
    assert!(ask::probability(&answers, "progress").is_none());
    assert!(ask::chosen(&answers, "yes").is_none());
    assert!(ask::Questions::default().is_empty());
}

#[test]
fn regions_split_at_the_first_level_that_divides_and_merge_the_tail() {
    let pool = (0..30)
        .map(|index| {
            node(
                &format!("b{index}"),
                "button",
                &["Click"],
                &["window", &format!("r{index}")],
                0.0,
            )
        })
        .collect::<Vec<_>>();
    let (level, regions) = ground::split(&pool, 0).unwrap();
    assert_eq!(level, 1);
    assert_eq!(regions.len(), ask::CAP);
    assert_eq!(regions.last().unwrap().0, "everything else");
    assert_eq!(regions.last().unwrap().1.len(), 30 - (ask::CAP - 1));
    let flat = vec![
        node("a", "button", &[], &[], 0.0),
        node("b", "button", &[], &[], 0.0),
    ];
    assert!(ground::split(&flat, 0).is_none());
}

#[test]
fn memory_matches_by_role_name_and_path_tail_and_replaces_old_hints() {
    let field = node(
        "Subject",
        "textfield",
        &["SetValue"],
        &["app", "window", "group"],
        0.0,
    );
    let hint = memory::remember("Mail", "The Subject", &field);
    assert_eq!(hint.key, "the subject");
    let pool = [field.clone()];
    assert!(memory::recall(std::slice::from_ref(&hint), "Mail", "the subject!", &pool).is_some());
    assert!(memory::recall(std::slice::from_ref(&hint), "Notes", "the subject", &pool).is_none());
    let moved = node("Subject", "textfield", &[], &["elsewhere"], 0.0);
    assert!(memory::recall(std::slice::from_ref(&hint), "Mail", "the subject", &[moved]).is_none());
    let mut hints = vec![hint.clone()];
    memory::learn(&mut hints, hint);
    assert_eq!(hints.len(), 1);
}
