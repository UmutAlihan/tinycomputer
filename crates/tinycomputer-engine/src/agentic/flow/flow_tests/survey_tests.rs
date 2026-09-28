//! The wide strategy's survey of a crowded screen, and the lookalikes it
//! offers once.

use super::*;

#[tokio::test]
async fn a_crowded_screen_is_surveyed_once_and_ranked_instead_of_narrowed() {
    let app = App::with(|sim| sim.extra_buttons = 60);
    let run = run_with(
        app,
        json!({"app": "Mail", "steps": ["open message 7"]}),
        wide,
        |id, question, sim| {
            if id == "done" {
                return Some(noul(if sim.clicks.contains(&"Message 7".to_owned()) {
                    0.95
                } else {
                    0.05
                }));
            }
            activate_moves(id, question, sim)
        },
    )
    .await;
    assert_eq!(run.result.steps[0].outcome, StepOutcome::Done);
    assert_eq!(run.app.sim().clicks, ["Message 7"]);
    assert_eq!(
        asked_prefix(&run.requests, "relevance_"),
        1,
        "one survey for the step's page shape, reused on later turns"
    );
    assert_eq!(
        asked(&run.requests, "region"),
        0,
        "no region-by-region narrowing"
    );
    assert!(run.result.steps[0].loops.contains(&FlowLoop::Survey));

    let survey = run
        .requests
        .iter()
        .find(|request| {
            request
                .questions
                .keys()
                .any(|id| id.starts_with("relevance_"))
        })
        .unwrap();
    assert!(
        survey
            .questions
            .keys()
            .any(|id| id.starts_with("distraction_"))
    );
    let turn = run
        .requests
        .iter()
        .find(|request| request.questions.contains_key("done"))
        .unwrap();
    let Question::Choice(first_group) = &turn.questions["group_activate_0"] else {
        panic!("a crowded pool is knocked out in the turn's own request");
    };
    let first_option = first_group.criteria["1"].as_ref().unwrap().to_string();
    assert!(
        first_option.contains("Region 1"),
        "the region the survey ranked highest is offered first: {first_option}"
    );
    let regions = &turn.state["screen"]["untrusted_accessibility_data"]["regions"];
    assert_eq!(regions[0]["relevance"], 1.0);
}

#[test]
fn survey_answers_follow_a_region_by_name_when_its_id_moves() {
    let form = node(
        "From",
        "textbox",
        &["Click"],
        &["webarea \"Book\"", "form \"Search\""],
        1.0,
    );
    let ad = node(
        "Deal",
        "link",
        &["Click"],
        &["webarea \"Book\"", "region \"Offers\""],
        2.0,
    );
    let before = Screen {
        app: "IndiGo".to_owned(),
        window: None,
        surface: "window".to_owned(),
        candidates: vec![form.clone(), ad.clone()],
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
    };
    let digest = super::view::digest(&before);
    let mut attention = super::survey::Attention::default();
    attention
        .relevance
        .insert(digest.region_of(0).unwrap().name.clone(), 0.9);
    attention
        .distractions
        .insert(digest.region_of(1).unwrap().name.clone());

    // A dropdown opens above both: every region's id moves on by one.
    let mut after = before.clone();
    after.candidates.insert(
        0,
        node(
            "Srinagar",
            "option",
            &["Click"],
            &["webarea \"Book\"", "listbox"],
            0.0,
        ),
    );
    let moved = super::view::digest(&after);
    let (relevance, distractions) = attention.for_digest(&moved);
    let form_id = moved.region_of(1).unwrap().id.clone();
    let ad_id = moved.region_of(2).unwrap().id.clone();
    assert_eq!(relevance.get(&form_id), Some(&0.9));
    assert!(distractions.contains(&ad_id));
    assert_eq!(relevance.len(), 1, "the new listbox has no answer yet");
}

#[test]
fn lookalikes_are_offered_once_and_the_first_in_page_order_is_kept() {
    let search = |order: usize, y: f64| Candidate {
        ref_id: format!("@s:search-{order}"),
        role: "combobox".to_owned(),
        available_actions: vec!["SetValue".to_owned()],
        bounds: Some(json!({"x": 10.0, "y": y})),
        path: vec!["main".to_owned(), "button \"destinationCity\"".to_owned()],
        order,
        ..Candidate::default()
    };
    let named = node("Pax Selection", "combobox", &["SetValue"], &["main"], 5.0);
    let pool = super::view::distinct(
        vec![
            search(1, 10.0),
            named.clone(),
            search(2, 20.0),
            search(3, 0.0),
        ],
        false,
    );
    let refs = pool.iter().map(|c| c.ref_id.as_str()).collect::<Vec<_>>();
    assert_eq!(refs, ["@s:search-1", "@s:Pax Selection"]);
}
