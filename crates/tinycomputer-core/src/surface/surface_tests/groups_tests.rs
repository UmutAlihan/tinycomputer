//! Tests for turning repeated cards and runs of leaf siblings into records.

use serde_json::json;

use super::clickable_screen;
use crate::surface::{Candidate, Screen, result_families, result_groups};

fn card(text: &[&str], button: &str, container: &str, order: usize) -> Vec<(Candidate, bool)> {
    let path = vec!["main".to_owned(), "list".to_owned(), container.to_owned()];
    let mut nodes = text
        .iter()
        .enumerate()
        .map(|(index, line)| {
            (
                Candidate {
                    role: "text".to_owned(),
                    value: Some(json!(line)),
                    path: path.clone(),
                    order: order + index,
                    ..Candidate::default()
                },
                false,
            )
        })
        .collect::<Vec<_>>();
    nodes.push((
        Candidate {
            ref_id: format!("e{order}"),
            role: "button".to_owned(),
            name: Some(button.to_owned()),
            available_actions: vec!["Click".to_owned()],
            path: path.clone(),
            order: order + 9,
            ..Candidate::default()
        },
        true,
    ));
    nodes
}

fn results(cards: Vec<Vec<(Candidate, bool)>>) -> Screen {
    let mut screen = clickable_screen();
    screen.candidates.clear();
    for (node, actionable) in cards.into_iter().flatten() {
        if actionable {
            screen.candidates.push(node);
        } else {
            screen.text_nodes.push(node);
        }
    }
    screen
}

#[test]
fn repeated_cards_become_records_with_their_opening_control() {
    let mut screen = results(vec![
        card(&["IndiGo", "₹6,840", "₹6,840"], "Select", "listitem #1", 0),
        card(
            &["Vistara", "  ₹7,210 "],
            "Select flight",
            "listitem #2",
            20,
        ),
        card(&["Air India", "₹8,050"], "Details", "listitem #3", 40),
    ]);
    // A second, heart-shaped control in the first card does not displace
    // the one that opens it.
    screen.candidates.push(Candidate {
        ref_id: "fav".to_owned(),
        role: "button".to_owned(),
        name: Some("Save to favourites".to_owned()),
        path: vec![
            "main".to_owned(),
            "list".to_owned(),
            "listitem #1".to_owned(),
        ],
        order: 5,
        ..Candidate::default()
    });
    let groups = result_groups(&screen);
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].label, "listitem #1");
    assert_eq!(
        groups[0].fields[..2],
        ["IndiGo", "₹6,840"],
        "repeats are dropped"
    );
    assert_eq!(groups[0].primary.as_ref().unwrap().ref_id, "e0");
    assert_eq!(groups[1].fields[1], "₹7,210");
    assert_eq!(
        groups[2].primary.as_ref().unwrap().name.as_deref(),
        Some("Details")
    );
}

#[test]
fn nothing_repeating_is_no_records() {
    assert!(result_groups(&clickable_screen()).is_empty());
    let single = results(vec![card(&["Only one"], "Select", "listitem #1", 0)]);
    assert!(result_groups(&single).is_empty());
    let mut unlabeled = results(vec![
        card(&["a"], "Select", "listitem", 0),
        card(&["b"], "Select", "listitem", 10),
    ]);
    assert!(result_groups(&unlabeled).is_empty());
    // Empty cards are dropped rather than offered as records.
    unlabeled = results(vec![
        card(&[], "", "row #1", 0),
        card(&[], "", "row #2", 10),
    ]);
    for node in &mut unlabeled.candidates {
        node.name = None;
    }
    assert!(result_groups(&unlabeled).is_empty());
}

#[test]
fn the_list_is_where_the_most_cards_repeat() {
    // Two filter chips repeat under a toolbar; five results repeat in the
    // list. The results win, and named ordinal labels are understood.
    let mut cards = (0..2)
        .map(|index| {
            card(
                &[&format!("chip {index}")],
                "Toggle",
                &format!("button \"Chip\" #{}", index + 1),
                index * 10,
            )
        })
        .collect::<Vec<_>>();
    for card_nodes in &mut cards {
        for (node, _) in card_nodes.iter_mut() {
            node.path[1] = "toolbar".to_owned();
        }
    }
    cards.extend((0..5).map(|index| {
        card(
            &[&format!("result {index}")],
            "Select",
            &format!("listitem #{}", index + 1),
            100 + index * 10,
        )
    }));
    let groups = result_groups(&results(cards));
    assert_eq!(groups.len(), 5);
    assert_eq!(groups[4].fields[0], "result 4");
}

/// A leaf of a desktop tree: no ordinal anywhere in its path.
fn leaf(role: &str, text: &str, parent: &str, order: usize) -> (Candidate, bool) {
    let actionable = role == "button";
    (
        Candidate {
            ref_id: if actionable {
                format!("e{order}")
            } else {
                String::new()
            },
            role: role.to_owned(),
            name: Some(text.to_owned()),
            available_actions: if actionable {
                vec!["Click".to_owned()]
            } else {
                Vec::new()
            },
            path: vec!["window".to_owned(), parent.to_owned()],
            order,
            ..Candidate::default()
        },
        actionable,
    )
}

#[test]
fn a_run_of_leaf_siblings_is_a_list_where_nothing_repeats_by_ordinal() {
    // A chat app: four chats as buttons in one pane, five messages as text
    // in the other, a lone heading, and a container that is not a leaf.
    let mut nodes = (0..4)
        .map(|index| leaf("button", &format!("Chat {index}"), "group \"Chats\"", index))
        .collect::<Vec<_>>();
    nodes.extend((0..5).map(|index| {
        leaf(
            "statictext",
            &format!("message {index}, 12:0{index}"),
            "group \"Messages\"",
            10 + index,
        )
    }));
    nodes.push(leaf("heading", "Today", "group \"Messages\"", 20));
    let mut parent = leaf("statictext", "not a leaf", "group \"Messages\"", 21);
    parent.0.children.push(Candidate::default());
    nodes.push(parent);
    let families = result_families(&results(vec![nodes]));
    assert_eq!(families.len(), 2, "the longest list first: {families:?}");
    assert_eq!(families[0].len(), 5);
    assert_eq!(families[0][4].fields, ["message 4, 12:04"]);
    assert_eq!(families[0][0].label, "statictext #1");
    assert!(families[0][0].primary.is_none(), "text opens nothing");
    assert_eq!(families[1][2].primary.as_ref().unwrap().ref_id, "e2");
    // Two of a kind are a label and its value, not a list.
    let pair = results(vec![vec![
        leaf("button", "OK", "group", 0),
        leaf("button", "Cancel", "group", 1),
    ]]);
    assert!(result_groups(&pair).is_empty());
}
