//! Tests for parsing a screen into regions: overlays in front, splitting large
//! groups, and keeping lists of different roles apart.

use serde_json::json;

use super::{node, results_page, screen, view};
use crate::surface::digest::{Digest, RegionKind, Rendering, digest};

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
