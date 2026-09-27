use super::{Candidate, Screen, fingerprint};
use serde_json::json;

fn screen(node: Candidate) -> Screen {
    Screen {
        app: "Example".to_owned(),
        window: None,
        window_id: None,
        surface: "window".to_owned(),
        root: None,
        candidates: Vec::new(),
        observed: vec![node],
    }
}

#[test]
fn actionability_and_child_count_change_fingerprint() {
    let mut node = Candidate {
        role: "button".to_owned(),
        name: Some("Play".to_owned()),
        ..Candidate::default()
    };
    let original = fingerprint(&screen(node.clone()));
    node.available_actions.push("click".to_owned());
    let actionable = fingerprint(&screen(node.clone()));
    assert_ne!(original, actionable);
    node.children_count = Some(2);
    assert_ne!(actionable, fingerprint(&screen(node)));
}

#[test]
fn large_accessibility_fields_produce_fixed_size_fingerprint() {
    let node = Candidate {
        role: "text".to_owned(),
        name: Some(format!("a{}", "é".repeat(100_000))),
        path: vec!["ancestor".repeat(100_000); 100],
        value: Some(json!({"text": "v".repeat(100_000)})),
        ..Candidate::default()
    };
    let digest = fingerprint(&screen(node));
    assert_eq!(digest.len(), 16);
    assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[test]
fn changes_after_the_first_512_visible_nodes_change_fingerprint() {
    let mut observed = (0..600)
        .map(|index| Candidate {
            role: "statictext".to_owned(),
            name: Some(format!("Status {index}")),
            ..Candidate::default()
        })
        .collect::<Vec<_>>();
    let before = Screen {
        observed: observed.clone(),
        ..screen(Candidate::default())
    };
    observed[599].name = Some("Finished".to_owned());
    let after = Screen {
        observed,
        ..before.clone()
    };
    assert_ne!(fingerprint(&before), fingerprint(&after));
}
