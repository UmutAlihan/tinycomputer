//! Tests for the screen digest: regions, what is in front, lists as cards,
//! noise, and spending a byte budget by relevance.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};

use crate::surface::{Candidate, Screen};

mod parse_tests;
mod render_tests;

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
