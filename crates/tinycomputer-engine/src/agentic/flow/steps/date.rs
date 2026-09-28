//! Reading a date option: whether it names a calendar day, and the words a
//! control must show to be that day.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use tinycomputer_bus::{
    ChooseStep, FlowAction, FlowLoop, FlowStopReason, IfStep, JevOperation, PickStep, ReadStep,
    RepeatStep, StepOutcome,
};
use tinycomputer_core::surface::{Group, result_families};
use tinycomputer_core::{Criterion, Record, rank};

use crate::workspace::BROWSER;

use crate::agentic::flow::{
    Ended, FlowRun, Halt, StepLog,
    act::{DONE, SCREEN_VIEW},
    ask::{self, Questions, chosen, condition, numbered},
    backend::{AgentBackend, deliver_text},
    escalate::Belief,
    ground::Grounded,
    memory::{learn, remember},
    validate::{MAX_REPEAT, substitute_safe},
    view::{Candidate, Screen, element_kind, is_destructive, label, target_payload},
};

use super::*;

/// The months a date picker is paged forward at most.
pub(super) const MAX_MONTHS: usize = 12;

/// Month names, as a date option spells them.
const MONTHS: &[&str] = &[
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// Whether `option` names a calendar day: a month name and a day number that
/// is a real day of that month (a year, when given, decides February's 28th
/// against its 29th). `February 31` or `April 31` names no such day, and is
/// read as ordinary autocomplete text instead of taking the calendar path.
pub(in crate::agentic::flow) fn looks_like_date(option: &str) -> bool {
    let lower = option.to_lowercase();
    let words = lower
        .split(|character: char| !character.is_alphanumeric())
        .collect::<Vec<_>>();
    let Some(month) = MONTHS.iter().position(|month| words.contains(month)) else {
        return false;
    };
    let Some(day) = words
        .iter()
        .find_map(|word| word.parse::<u8>().ok().filter(|day| (1..=31).contains(day)))
    else {
        return false;
    };
    let year = words.iter().find_map(|word| {
        word.parse::<u16>()
            .ok()
            .filter(|year| (1900..=2100).contains(year))
    });
    day <= days_in_month(month, year)
}

/// How many days `month` (0 = January, from [`MONTHS`]) has; February is
/// taken as 29 when no `year` narrows it, so a bare "29 February" is still
/// treated as a date worth paging to.
fn days_in_month(month: usize, year: Option<u16>) -> u8 {
    match month {
        0 | 2 | 4 | 6 | 7 | 9 | 11 => 31,
        3 | 5 | 8 | 10 => 30,
        _ => {
            if year.is_none_or(is_leap_year) {
                29
            } else {
                28
            }
        }
    }
}

/// Whether `year` is a leap year in the Gregorian calendar.
fn is_leap_year(year: u16) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Whether a control's label says only that it shows the next month; a
/// date field whose label lists the whole calendar says much more.
pub(super) fn is_next_month(name: &str) -> bool {
    let words = plain(name);
    words.contains("next month") && words.split(' ').count() <= 4
}

/// The day, month, and year (when given) a date option names, as words.
pub(super) fn date_words(option: &str) -> Vec<String> {
    plain(option)
        .split(' ')
        .filter(|word| {
            MONTHS.contains(word)
                || word.parse::<u16>().is_ok_and(|number| {
                    (1..=31).contains(&number) || (1900..=2100).contains(&number)
                })
        })
        .map(str::to_owned)
        .collect()
}
