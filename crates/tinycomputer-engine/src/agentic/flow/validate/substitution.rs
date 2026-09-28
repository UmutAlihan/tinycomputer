//! `${name}` references: finding them, substituting them once without
//! rescanning, and normalizing text for comparison.

use std::collections::{BTreeMap, BTreeSet};



/// Every `${name}` referenced in `text`.
pub(in crate::agentic::flow) fn references(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            break;
        };
        names.push(after[..end].to_owned());
        rest = &after[end + 1..];
    }
    names
}

/// Replaces every defined `${name}` in `text`; undefined ones are left as-is.
///
/// Scans `text` once, left to right, and never rescans a value it just
/// substituted in. `read` steps store untrusted screen text in `vars`, so a
/// value that itself looks like `${other}` must stand as literal text rather
/// than expand into `other`'s value.
pub(in crate::agentic::flow) fn substitute(text: &str, vars: &BTreeMap<String, String>) -> String {
    substitute_with(text, |name| vars.get(name).map(String::as_str))
}

/// [`substitute`], but a name in `facts` is treated as undefined and left as
/// literal `${name}` rather than expanded.
///
/// This is the substitution every step goes through except an `enter` step's
/// typed value: the runtime's backstop against a fact's value ever reaching
/// Jev — directly, or later as state on a subsequent step — even if
/// validation somehow let a `${fact}` reference through.
pub(in crate::agentic::flow) fn substitute_safe(
    text: &str,
    vars: &BTreeMap<String, String>,
    facts: &BTreeSet<String>,
) -> String {
    substitute_with(text, |name| {
        if facts.contains(name) {
            None
        } else {
            vars.get(name).map(String::as_str)
        }
    })
}

pub(super) fn substitute_with<'a>(text: &'a str, resolve: impl Fn(&str) -> Option<&'a str>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        match resolve(name) {
            Some(value) => out.push_str(value),
            None => out.push_str(&rest[start..=start + 2 + end]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// A stable key for a step or slot: lower-case words, punctuation dropped.
pub(in crate::agentic::flow) fn normalize(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
