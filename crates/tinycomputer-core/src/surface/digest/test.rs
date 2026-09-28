//! Tests for the screen digest: regions, what is in front, lists as cards,
//! noise, and spending a byte budget by relevance.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::{Digest, RegionKind, Rendering, digest};
use crate::surface::{Candidate, Screen};

fn node(name: &str, role: &str, path: &[&str], order: usize) -> Candidate {
    Candidate {
        ref_id: format!("@e:{name}"),
        role: role.to_owned(),
        name: Some(name.to_owned()),
        available_actions: vec!["Click".to_owned()],
        path: path.iter().map(|label| (*label).to_owned()).collect(),
        order,
        ..Candidate::default()
    }
}

fn screen(candidates: Vec<Candidate>, text_nodes: Vec<Candidate>) -> Screen {
    Screen {
        app: "Shop".to_owned(),
        window: Some("Results".to_owned()),
        surface: "window".to_owned(),
        candidates,
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes,
    }
}

/// A results page: a search toolbar, a cookie banner, a footer, and a list
/// of `cards` flight cards each with text and a "Select" button.
fn results_page(cards: usize) -> Screen {
    let root = "webarea \"Flights\"";
    let mut candidates = vec![
        node("Search", "button", &[root, "form \"Search\""], 1),
        node("From", "textbox", &[root, "form \"Search\""], 2),
        node(
            "Accept all",
            "button",
            &[root, "region \"Cookie consent\""],
            3,
        ),
        node(
            "Essential only",
            "button",
            &[root, "region \"Cookie consent\""],
            4,
        ),
    ];
    let mut text_nodes = Vec::new();
    for card in 0..cards {
        let item = format!("listitem #{}", card + 1);
        let path = [root, "list \"Results\"", item.as_str()];
        let order = 100 + card * 10;
        text_nodes.push(Candidate {
            role: "text".to_owned(),
            value: Some(json!(format!("Airline {card}"))),
            path: path.iter().map(|label| (*label).to_owned()).collect(),
            order,
            ..Candidate::default()
        });
        candidates.push(Candidate {
            ref_id: format!("@e:select-{card}"),
            ..node("Select", "button", &path, order + 1)
        });
    }
    for link in 0..30 {
        candidates.push(node(
            &format!("Legal {link}"),
            "link",
            &[root, "contentinfo"],
            10_000 + link,
        ));
    }
    screen(candidates, text_nodes)
}

fn view(rendered: &Value) -> &Value {
    &rendered["untrusted_accessibility_data"]
}

#[test]
fn an_overlay_is_its_own_region_in_front() {
    let page = results_page(3);
    let digest = digest(&page);
    let front = digest.front().collect::<Vec<_>>();
    assert_eq!(front.len(), 1);
    assert_eq!(front[0].id, "r1", "whatever is in front comes first");
    assert_eq!(front[0].members, [2, 3]);
    assert!(front[0].name.contains("Cookie consent"));

    let rendered = digest.render(
        &page,
        &Rendering {
            budget: 10_000,
            ..Rendering::default()
        },
    );
    let in_front = &view(&rendered)["in_front"][0];
    assert_eq!(in_front["elements"][0], "e2 button \"Accept all\"");
}

#[test]
fn a_sheet_by_role_is_in_front_but_a_titled_root_is_not() {
    let page = screen(
        vec![
            node("Keep Editing", "button", &["sheet"], 1),
            node("Subscribe", "button", &["window \"Newsletter\""], 2),
        ],
        Vec::new(),
    );
    let digest = digest(&page);
    assert_eq!(digest.regions[0].kind, RegionKind::Front);
    assert_eq!(digest.regions[0].members, [0]);
    assert_eq!(
        digest.region_of(1).unwrap().kind,
        RegionKind::Content,
        "a window titled with an overlay word is the page itself"
    );
}

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
    assert_eq!(lines.len(), super::LIST_CARDS + 1);
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
fn a_large_group_splits_one_level_deeper_and_names_regions_by_their_ancestors() {
    let root = "window \"Mail\"";
    let mut candidates = Vec::new();
    for index in 0..40 {
        let pane = if index < 20 {
            "group \"Mailboxes\""
        } else {
            "group \"Messages\""
        };
        candidates.push(node(
            &format!("Item {index}"),
            "row",
            &[root, "splitgroup", pane],
            index,
        ));
    }
    let page = screen(candidates, Vec::new());
    let digest = digest(&page);
    assert_eq!(digest.regions.len(), 2);
    assert_eq!(digest.regions[0].name, "splitgroup > group \"Mailboxes\"");
    assert_eq!(digest.regions[1].members.len(), 20);
    assert_eq!(
        digest.layout(),
        "Content:splitgroup > group \"Mailboxes\"|Content:splitgroup > group \"Messages\""
    );
}

#[test]
fn an_empty_screen_digests_to_nothing_and_renders_an_empty_map() {
    let page = screen(Vec::new(), Vec::new());
    let digest = digest(&page);
    assert_eq!(digest, Digest::default());
    let rendered = digest.render(&page, &Rendering::default());
    assert_eq!(
        rendered,
        json!({"untrusted_accessibility_data": {"regions": []}})
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
    assert_eq!(super::clip("abcdef", 3), "abc…");
    assert_eq!(super::clip("abc", 3), "abc");
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
fn sibling_containers_of_different_roles_never_share_one_card_list() {
    let root = "webarea \"App\"";
    let page = screen(
        vec![
            node("Row A", "statictext", &[root, "listitem #1"], 1),
            node("Row B", "statictext", &[root, "tab #1"], 2),
        ],
        Vec::new(),
    );
    let digest = digest(&page);
    assert!(
        digest.regions.iter().all(|region| region.list.is_none()),
        "a listitem and a tab, both #1, are not one repeated list: {:?}",
        digest.regions
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
        total < super::COLLAPSED_SLACK + super::SUMMARY_CHARS * 4,
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
