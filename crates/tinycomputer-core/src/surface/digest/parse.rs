//! Parsing a screen into regions: overlays first, then the page split by the
//! ancestors its elements share, with lists of repeated cards kept whole.

use std::collections::BTreeMap;

use super::{Candidate, Screen, groups};
use super::{Digest, Region, RegionKind};
use super::{FRONT_ROLES, FRONT_WORDS, MAX_DEPTH, NOISE_WORDS, REGION_SIZE};

/// Parses `screen` into regions.
#[must_use]
pub fn digest(screen: &Screen) -> Digest {
    let mut front: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut front_order = Vec::new();
    let mut rest = Vec::new();
    for (index, candidate) in screen.candidates.iter().enumerate() {
        if let Some(overlay) = overlay_of(candidate) {
            if !front.contains_key(&overlay) {
                front_order.push(overlay.clone());
            }
            front.entry(overlay).or_default().push(index);
        } else {
            rest.push(index);
        }
    }
    let mut regions = Vec::new();
    for overlay in front_order {
        let members = front.remove(&overlay).unwrap_or_default();
        regions.push(Region {
            id: String::new(),
            name: region_name(screen, &members),
            kind: RegionKind::Front,
            members,
            list: None,
        });
    }
    let mut content = Vec::new();
    split(screen, rest, 0, &mut content);
    regions.extend(content);
    for (position, region) in regions.iter_mut().enumerate() {
        region.id = format!("r{}", position + 1);
    }
    Digest { regions }
}

/// Splits `members` into regions of at most [`REGION_SIZE`], one ancestor
/// level at a time, keeping a list of repeated cards whole and apart from
/// whatever sits beside it.
fn split(screen: &Screen, members: Vec<usize>, level: usize, out: &mut Vec<Region>) {
    if members.is_empty() {
        return;
    }
    if let Some(list) = list_at(screen, &members, level) {
        out.push(region(screen, members, Some(list)));
        return;
    }
    if level >= MAX_DEPTH || (members.len() <= REGION_SIZE && !holds_a_list(screen, &members)) {
        out.push(region(screen, members, None));
        return;
    }
    let mut groups: Vec<(Option<String>, Vec<usize>)> = Vec::new();
    for index in members {
        let key = screen.candidates[index].path.get(level).cloned();
        match groups.iter_mut().find(|(existing, _)| *existing == key) {
            Some((_, group)) => group.push(index),
            None => groups.push((key, vec![index])),
        }
    }
    for (_, group) in groups {
        split(screen, group, level + 1, out);
    }
}

/// The list `members` form at `level`: every member sits under the same
/// ancestors and then under one of at least two ordinal containers of the
/// same role (`listitem #3`, never a `listitem #1` next to a `tab #2`)
/// there.
fn list_at(screen: &Screen, members: &[usize], level: usize) -> Option<(usize, Vec<String>)> {
    let first = &screen.candidates[*members.first()?].path;
    let parent = first.get(..level)?;
    let mut containers = Vec::new();
    let mut role = None;
    for index in members {
        let path = &screen.candidates[*index].path;
        let container = path.get(level)?;
        let (container_role, _) = groups::ordinal(container)?;
        if path.get(..level)? != parent || *role.get_or_insert(container_role) != container_role {
            return None;
        }
        if !containers.contains(&container) {
            containers.push(container);
        }
    }
    (containers.len() >= 2).then(|| (level, parent.to_vec()))
}

/// Whether two or more of `members` sit in different ordinal containers
/// under the same ancestors: a list that needs a region of its own.
fn holds_a_list(screen: &Screen, members: &[usize]) -> bool {
    let mut seen: BTreeMap<(usize, &[String]), &String> = BTreeMap::new();
    for index in members {
        let path = &screen.candidates[*index].path;
        for (level, label) in path.iter().enumerate() {
            if groups::ordinal(label).is_none() {
                continue;
            }
            match seen.get(&(level, &path[..level])) {
                Some(other) if *other != label => return true,
                Some(_) => {}
                None => {
                    seen.insert((level, &path[..level]), label);
                }
            }
        }
    }
    false
}

fn region(screen: &Screen, members: Vec<usize>, list: Option<(usize, Vec<String>)>) -> Region {
    let name = region_name(screen, &members);
    // The ancestor labels every member shares (the same prefix `region_name`
    // reports), not just the first member's whole path in document order: a
    // region's noise verdict must depend on the structure every member of it
    // sits under, not on which member the accessibility tree happened to
    // list first.
    let noisy = shared_path(screen, &members).is_some_and(|shared| {
        // The root is the window or page itself: its title says nothing
        // about which part of it is noise.
        shared
            .iter()
            .skip(1)
            .any(|label| words(label).any(|word| NOISE_WORDS.contains(&word.as_str())))
    });
    Region {
        id: String::new(),
        name,
        kind: if noisy {
            RegionKind::Noise
        } else {
            RegionKind::Content
        },
        members,
        list,
    }
}

/// The ancestor labels every one of `members` shares, from the root; `None`
/// when they share none (or there are no members).
fn shared_path(screen: &Screen, members: &[usize]) -> Option<Vec<String>> {
    let paths = members
        .iter()
        .filter_map(|index| screen.candidates.get(*index))
        .map(|candidate| &candidate.path)
        .collect::<Vec<_>>();
    let first = paths.first()?;
    let shared = (0..first.len())
        .take_while(|level| {
            paths
                .iter()
                .all(|path| path.get(*level) == first.get(*level))
        })
        .count();
    (shared > 0).then(|| first[..shared].to_vec())
}

/// The last two labels every member's path shares, or `top level`.
fn region_name(screen: &Screen, members: &[usize]) -> String {
    shared_path(screen, members).map_or_else(
        || "top level".to_owned(),
        |shared| shared[shared.len().saturating_sub(2)..].join(" > "),
    )
}

/// The overlay `candidate` sits inside, named by its ancestor labels down to
/// the overlay, or `None` when it is part of the page itself.
fn overlay_of(candidate: &Candidate) -> Option<String> {
    candidate
        .path
        .iter()
        .enumerate()
        .position(|(level, label)| {
            let role = label.split(' ').next().unwrap_or_default().to_lowercase();
            // A root titled "Newsletter" is the page, not something over it.
            FRONT_ROLES.contains(&role.as_str())
                || (level > 0 && words(label).any(|word| FRONT_WORDS.contains(&word.as_str())))
        })
        .map(|at| candidate.path[at.saturating_sub(1)..=at].join(" > "))
}

fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}
