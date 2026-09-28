//! Tests for rendering a digest: lists as cards, noise, relevance, byte budgets,
//! and when values are shown.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use super::{node, results_page, screen, view};
use crate::surface::Candidate;
use crate::surface::digest::{RegionKind, Rendering, digest};

#[test]
fn a_long_list_is_one_region_shown_as_cards() {
    let page = results_page(20);
    let digest = digest(&page);
    let list = digest
        .regions
        .iter()
        .find(|region| region.list.is_some())
        .expect("the result list stays one region");
    assert_eq!(list.members.len(), 20);
    assert!(list.name.ends_with("list \"Results\""));

    let rendered = digest.render(
        &page,
        &Rendering {
            budget: 100_000,
            ..Rendering::default()
        },
    );
    let lines = view(&rendered)["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|region| region["id"] == list.id)
        .unwrap()["elements"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        lines[0],
        "card 1: Airline 0 · Select → e4 button \"Select\""
    );
    assert_eq!(lines.len(), crate::surface::digest::LIST_CARDS + 1);
    assert_eq!(lines.last().unwrap(), "and 8 more cards like these");
}

#[test]
fn noise_is_collapsed_unless_it_is_relevant() {
    let page = results_page(2);
    let digest = digest(&page);
    let footer = digest
        .regions
        .iter()
        .find(|region| region.kind == RegionKind::Noise)
        .expect("the footer is noise");
    assert_eq!(footer.members.len(), 30);

    let rendered = digest.render(
        &page,
        &Rendering {
            budget: 100_000,
            ..Rendering::default()
        },
    );
    let collapsed = view(&rendered)["collapsed"].as_array().unwrap();
    assert_eq!(collapsed.len(), 1);
    let line = collapsed[0].as_str().unwrap();
    assert!(line.starts_with(&format!("{} ", footer.id)));
    assert!(line.contains("30 elements, likely noise, e.g. link \"Legal 0\""));

    let relevance = BTreeMap::from([(footer.id.clone(), 0.9)]);
    let rescued = digest.render(
        &page,
        &Rendering {
            budget: 100_000,
            relevance: Some(&relevance),
            ..Rendering::default()
        },
    );
    assert!(view(&rescued).get("collapsed").is_none());
    assert_eq!(view(&rescued)["regions"][0]["id"], footer.id);
    assert_eq!(view(&rescued)["regions"][0]["relevance"], 0.9);
}

#[test]
fn a_tight_budget_keeps_the_relevant_region_and_collapses_the_rest() {
    let page = results_page(4);
    let digest = digest(&page);
    let form = digest.region_of(0).unwrap().id.clone();
    let list = digest.region_of(4).unwrap().id.clone();
    let relevance = BTreeMap::from([(list.clone(), 1.0), (form.clone(), 0.1)]);
    let rendered = digest.render(
        &page,
        &Rendering {
            budget: 400,
            relevance: Some(&relevance),
            ..Rendering::default()
        },
    );
    let shown = view(&rendered)["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|region| region["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(shown, std::slice::from_ref(&list));
    let collapsed = view(&rendered)["collapsed"].as_array().unwrap();
    assert!(
        collapsed
            .iter()
            .any(|line| line.as_str().unwrap().starts_with(&format!("{form} ")))
    );

    let ranked = digest.ranked(&Rendering {
        relevance: Some(&relevance),
        ..Rendering::default()
    });
    assert_eq!(&ranked[..2], [2, 3], "the overlay ranks first");
    assert_eq!(ranked[2], 4, "then the most relevant region");
    assert!(
        ranked.iter().rev().take(30).all(|index| *index >= 8),
        "noise ranks last"
    );
}

#[test]
fn a_distraction_is_collapsed_and_ranked_last() {
    let page = results_page(2);
    let digest = digest(&page);
    let form = digest.region_of(0).unwrap().id.clone();
    let distractions = BTreeSet::from([form.clone()]);
    let rendering = Rendering {
        budget: 100_000,
        distractions: Some(&distractions),
        ..Rendering::default()
    };
    let ranked = digest.ranked(&rendering);
    assert!(ranked.iter().position(|index| *index == 0).unwrap() > 3);
    let rendered = digest.render(&page, &rendering);
    assert!(
        view(&rendered)["collapsed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line.as_str().unwrap().starts_with(&format!("{form} ")))
    );
}

#[test]
fn values_are_shown_only_when_allowed_and_long_cards_are_clipped() {
    let mut field = node("To", "textfield", &["window \"New\""], 1);
    field.value = Some(json!("sam@example.com"));
    field.states = vec!["focused".to_owned()];
    let page = screen(vec![field], Vec::new());
    let digest = digest(&page);
    let hidden = digest.render(
        &page,
        &Rendering {
            budget: 1_000,
            ..Rendering::default()
        },
    );
    assert_eq!(
        view(&hidden)["regions"][0]["elements"][0],
        "e0 textfield \"To\" [focused]"
    );
    let shown = digest.render(
        &page,
        &Rendering {
            budget: 1_000,
            include_values: true,
            ..Rendering::default()
        },
    );
    assert_eq!(
        view(&shown)["regions"][0]["elements"][0],
        "e0 textfield \"To\" = \"sam@example.com\" [focused]"
    );
    assert_eq!(crate::surface::digest::render::clip("abcdef", 3), "abc…");
    assert_eq!(crate::surface::digest::render::clip("abc", 3), "abc");
}

#[test]
fn a_card_s_rich_text_body_is_gated_on_include_values_but_its_name_is_not() {
    let root = "webarea \"Docs\"";
    let mut candidates = Vec::new();
    let mut text_nodes = Vec::new();
    for item in 1..=2 {
        let order = item * 10;
        let path = [root, "list \"Docs\"", &format!("listitem #{item}")];
        text_nodes.push(node(&format!("Doc {item}"), "text", &path, order));
        text_nodes.push(Candidate {
            role: "text".to_owned(),
            value: Some(json!(format!("body text of doc {item}"))),
            path: vec![
                root.to_owned(),
                "list \"Docs\"".to_owned(),
                format!("listitem #{item}"),
                "textbox \"Body\"".to_owned(),
            ],
            order: order + 1,
            ..Candidate::default()
        });
        candidates.push(node(&format!("Open {item}"), "button", &path, order + 2));
    }
    let page = screen(candidates, text_nodes);
    let digest = digest(&page);
    let list = digest
        .regions
        .iter()
        .find(|region| region.list.is_some())
        .expect("the doc list stays one region");

    let hidden = digest.render(
        &page,
        &Rendering {
            budget: 100_000,
            ..Rendering::default()
        },
    );
    let lines = view(&hidden)["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|region| region["id"] == list.id)
        .unwrap()["elements"]
        .as_array()
        .unwrap()
        .clone();
    let hidden_line = lines[0].as_str().unwrap();
    assert!(
        hidden_line.starts_with("card 1: Doc 1 · Open 1 →"),
        "the card's name still shows: {hidden_line}"
    );
    assert!(
        !hidden_line.contains("body text"),
        "a rich-text body must not leak when include_values is false: {hidden_line}"
    );

    let shown = digest.render(
        &page,
        &Rendering {
            budget: 100_000,
            include_values: true,
            ..Rendering::default()
        },
    );
    let shown_lines = view(&shown)["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|region| region["id"] == list.id)
        .unwrap()["elements"]
        .as_array()
        .unwrap()
        .clone();
    let shown_line = shown_lines[0].as_str().unwrap();
    assert!(
        shown_line.contains("body text of doc 1"),
        "the rich-text body must show when include_values is true: {shown_line}"
    );
}

#[test]
fn many_collapsed_regions_stop_growing_the_digest_past_its_budget() {
    let mut candidates = Vec::new();
    for group in 0..40 {
        let container = format!("group \"G{group}\"");
        for item in 0..2 {
            candidates.push(node(
                &format!("g{group}i{item}"),
                "button",
                &["main", container.as_str()],
                group * 2 + item,
            ));
        }
    }
    let page = screen(candidates, Vec::new());
    let digest = digest(&page);
    assert!(
        digest.regions.len() > 10,
        "the page splits into many small regions to collapse"
    );
    let rendered = digest.render(
        &page,
        &Rendering {
            budget: 100,
            ..Rendering::default()
        },
    );
    let collapsed = view(&rendered)["collapsed"].as_array().unwrap().clone();
    let total: usize = collapsed
        .iter()
        .map(|line| line.as_str().unwrap().len())
        .sum();
    assert!(
        total < crate::surface::digest::COLLAPSED_SLACK + crate::surface::digest::SUMMARY_CHARS * 4,
        "collapsed summaries must stop growing well past the budget, got {total} bytes over {} lines",
        collapsed.len()
    );
    assert!(
        collapsed
            .last()
            .unwrap()
            .as_str()
            .unwrap()
            .contains("more regions not shown"),
        "the omitted tail is reported once instead of per region: {collapsed:?}"
    );
}
