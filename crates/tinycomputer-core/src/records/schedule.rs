//! Reading clock times, durations, and stop counts.

/// Reads a clock time as minutes after midnight: `06:45`, `6:45 PM`, `18:05`.
///
/// ```
/// use tinycomputer_core::parse_clock;
///
/// assert_eq!(parse_clock("Departs 6:45 PM"), Some(18 * 60 + 45));
/// ```
#[must_use]
pub fn parse_clock(text: &str) -> Option<u32> {
    let lower = text.to_ascii_lowercase();
    let colon = lower.find(':')?;
    let hours: u32 = lower[..colon]
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>()
        .parse()
        .ok()?;
    let minutes: u32 = lower.get(colon + 1..colon + 3)?.parse().ok()?;
    if minutes > 59 {
        return None;
    }
    let rest = lower[colon + 3..].trim_start();
    let hours = match (
        rest.starts_with("pm") || rest.starts_with("p.m"),
        rest.starts_with("am") || rest.starts_with("a.m"),
    ) {
        (true, _) if hours < 12 => hours + 12,
        (_, true) if hours == 12 => 0,
        _ => hours,
    };
    (hours < 24).then_some(hours * 60 + minutes)
}

/// Reads a duration as minutes: `2h 15m`, `2 hr 15 min`, `135 min`, `1h`.
///
/// ```
/// use tinycomputer_core::parse_duration;
///
/// assert_eq!(parse_duration("2h 15m"), Some(135));
/// ```
#[must_use]
pub fn parse_duration(text: &str) -> Option<u32> {
    const HOURS: &[&str] = &["h", "hr", "hrs", "hour", "hours"];
    const MINUTES: &[&str] = &["m", "min", "mins", "minute", "minutes"];
    let lower = text.to_ascii_lowercase();
    let mut total = 0;
    let mut found = false;
    let mut rest = lower.as_str();
    while let Some(start) = rest.find(|character: char| character.is_ascii_digit()) {
        let after = &rest[start..];
        let length = after
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(after.len());
        let value: u32 = after[..length].parse().ok()?;
        let unit_text = after[length..].trim_start();
        let unit_length = unit_text
            .find(|character: char| !character.is_ascii_alphabetic())
            .unwrap_or(unit_text.len());
        let unit = &unit_text[..unit_length];
        if HOURS.contains(&unit) {
            total += value * 60;
            found = true;
        } else if MINUTES.contains(&unit) {
            total += value;
            found = true;
        }
        rest = &after[length..];
    }
    found.then_some(total)
}

/// Reads how many stops a journey makes: `Nonstop` and `Direct` are zero.
///
/// ```
/// use tinycomputer_core::parse_stops;
///
/// assert_eq!(parse_stops("Non-stop"), Some(0));
/// assert_eq!(parse_stops("1 stop · DEL"), Some(1));
/// ```
#[must_use]
pub fn parse_stops(text: &str) -> Option<u32> {
    let lower = text.to_ascii_lowercase().replace('-', "");
    if lower.contains("nonstop") || lower.contains("direct") {
        return Some(0);
    }
    let at = lower.find("stop")?;
    lower[..at]
        .split_whitespace()
        .next_back()
        .and_then(|word| word.parse().ok())
}
