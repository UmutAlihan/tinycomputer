//! A screen as a map rather than a list: the agent-friendly view a decision
//! model reads.
//!
//! A raw observation is a flat list of every actionable element in document
//! order. A person reads a screen differently: whatever pops up in front
//! first, then the part of the page the task is about, while the footer, the
//! ads, and the twenty identical "Select" buttons of a result list fade into
//! the background. [`digest`] does that parsing deterministically:
//!
//! - elements are grouped into **regions** by the ancestors they share, and
//!   a region too large to take in is split one level deeper;
//! - a **dialog, sheet, popover, or consent banner** is its own region,
//!   placed first and marked `in_front`;
//! - a **list of repeated cards** stays one region and is shown as one line
//!   per card — its text and its "open this" control — instead of every
//!   element inside every card;
//! - **advertising, sponsored content, footers, and legal text** are marked
//!   as noise and collapsed to one line.
//!
//! [`Digest::render`] spends a byte budget region by region, most relevant
//! first, so a crowded page shows the part that matters in full and the rest
//! as one-line summaries. Every element keeps a short id (`e12`, its index in
//! `Screen::candidates`) that maps straight back to the element. Everything
//! read from the screen is wrapped as `untrusted_accessibility_data`.

mod types;

pub use types::{Digest, Region, RegionKind, Rendering};

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::{Candidate, Screen, element_line, groups, label};

/// Most elements a region holds before it is split one level deeper.
const REGION_SIZE: usize = 24;
/// Deepest ancestor level regions are split on.
const MAX_DEPTH: usize = 10;
/// Cards of a list shown one line each before the rest are counted.
const LIST_CARDS: usize = 12;
/// Longest card line, in characters.
const CARD_CHARS: usize = 160;
/// Example labels a collapsed region names.
const EXAMPLES: usize = 5;
/// Longest a collapsed region's one-line summary is let to run, so a long
/// region name or many long example labels cannot inflate it unboundedly.
const SUMMARY_CHARS: usize = 200;
/// How far `render` lets collapsed summaries push `spent` past
/// `rendering.budget` before it stops adding them and reports a count of the
/// rest instead: enough slack for one more summary or two, not an unbounded
/// tail of them.
const COLLAPSED_SLACK: usize = SUMMARY_CHARS * 2;
/// Roles whose subtree is in front of the page.
const FRONT_ROLES: &[&str] = &["sheet", "dialog", "alertdialog", "alert", "popover"];
/// Words in a region's labels that mark an overlay a person must deal with.
const FRONT_WORDS: &[&str] = &[
    "cookie",
    "cookies",
    "consent",
    "gdpr",
    "newsletter",
    "subscribe",
    "popup",
    "modal",
    "overlay",
];
/// Words in a region's labels that mark it as noise.
const NOISE_WORDS: &[&str] = &[
    "ads",
    "advert",
    "advertisement",
    "advertising",
    "sponsored",
    "promoted",
    "promotion",
    "footer",
    "contentinfo",
    "copyright",
];

mod parse;
mod render;

pub use parse::digest;

#[cfg(test)]
mod digest_tests;
