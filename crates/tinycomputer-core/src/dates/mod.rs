//! Calendar dates as a person gives them, and as a form wants them typed.
//!
//! A caller writes a date of birth as `2000-01-01` or `1 January 2000`; a
//! booking form's masked field wants `01-01-2000` and says so beside it
//! ("enter date of birth in DD-MM-YYYY format"). [`reformat_date`] turns one
//! into the other, so a value is typed the way the field reads it instead of
//! being mangled by its input mask.

/// A calendar date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    /// The year, such as 2000.
    pub year: u32,
    /// The month, 1 to 12.
    pub month: u32,
    /// The day of the month, 1 to 31.
    pub day: u32,
}

const MONTHS: [&str; 12] = [
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

/// Date layouts a form may ask for, as it spells them, most specific first.
const PATTERNS: &[&str] = &[
    "DD-MM-YYYY",
    "DD/MM/YYYY",
    "DD.MM.YYYY",
    "MM-DD-YYYY",
    "MM/DD/YYYY",
    "YYYY-MM-DD",
    "YYYY/MM/DD",
    "DD-MM-YY",
    "DD/MM/YY",
    "MM/DD/YY",
];

/// The date `text` names without ambiguity: `2000-01-01`, `2000/1/1`,
/// `1 January 2000`, `1 Jan 2000`, `January 1, 2000`. A form like
/// `01/02/2000`, which is January or February by country, is not read.
///
/// ```
/// use tinycomputer_core::{Date, parse_date};
///
/// let day = Date { year: 2000, month: 1, day: 31 };
/// assert_eq!(parse_date("2000-01-31"), Some(day));
/// assert_eq!(parse_date("31 Jan 2000"), Some(day));
/// assert_eq!(parse_date("January 31, 2000"), Some(day));
/// assert_eq!(parse_date("01/02/2000"), None);
/// ```
#[must_use]
pub fn parse_date(text: &str) -> Option<Date> {
    let words = text
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    let number = |word: &str| word.parse::<u32>().ok();
    let month_of = |word: &str| {
        (word.len() >= 3)
            .then(|| MONTHS.iter().position(|month| month.starts_with(word)))
            .flatten()
            .and_then(|index| u32::try_from(index + 1).ok())
    };
    let date = match words.as_slice() {
        [year, month, day] if year.len() == 4 && month_of(month).is_none() => Date {
            year: number(year)?,
            month: number(month)?,
            day: number(day)?,
        },
        [day, month, year] if month_of(month).is_some() => Date {
            year: number(year)?,
            month: month_of(month)?,
            day: number(day)?,
        },
        [month, day, year] if month_of(month).is_some() => Date {
            year: number(year)?,
            month: month_of(month)?,
            day: number(day)?,
        },
        _ => return None,
    };
    ((1..=12).contains(&date.month) && (1..=31).contains(&date.day) && date.year >= 1000)
        .then_some(date)
}

/// The layout the first of `hints` that names one asks for, such as
/// `DD-MM-YYYY` from "enter date of birth in (DD-MM-YYYY) format".
#[must_use]
pub fn date_pattern<'a>(hints: impl IntoIterator<Item = &'a str>) -> Option<&'static str> {
    hints.into_iter().find_map(|hint| {
        let hint = hint.to_uppercase();
        PATTERNS
            .iter()
            .find(|pattern| hint.contains(*pattern))
            .copied()
    })
}

/// `value` retyped in the layout `hints` ask for, when `value` is a date and
/// a hint names a layout; `None` leaves it as given.
///
/// ```
/// use tinycomputer_core::reformat_date;
///
/// let hints = ["Date of Birth", "Please enter date of birth in (DD-MM-YYYY) format"];
/// assert_eq!(reformat_date("2000-01-31", hints).as_deref(), Some("31-01-2000"));
/// assert_eq!(reformat_date("Asha", hints), None);
/// ```
#[must_use]
pub fn reformat_date<'a>(value: &str, hints: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let date = parse_date(value)?;
    let pattern = date_pattern(hints)?;
    Some(
        pattern
            .replace("YYYY", &format!("{:04}", date.year))
            .replace("YY", &format!("{:02}", date.year % 100))
            .replace("MM", &format!("{:02}", date.month))
            .replace("DD", &format!("{:02}", date.day)),
    )
}

#[cfg(test)]
mod dates_tests;
