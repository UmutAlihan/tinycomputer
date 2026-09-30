//! Shared text utilities for accessibility value parsing.

#[must_use]
/// Keep the last `max_chars` characters of `text`.
pub fn truncate_tail(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    chars[chars.len() - max_chars..].iter().collect()
}

#[must_use]
/// Trim an accessibility string value; the literal `missing value` becomes empty.
pub fn normalize_ax_value(raw: &str) -> String {
    let v = raw.trim();
    if v.eq_ignore_ascii_case("missing value") {
        String::new()
    } else {
        v.to_string()
    }
}

#[must_use]
/// Parse an accessibility number (comma or dot decimal) to a rounded `i32`; `None` if empty, non-finite, or out of range.
pub fn parse_ax_number(raw: &str) -> Option<i32> {
    let trimmed = normalize_ax_value(raw);
    if trimmed.is_empty() {
        return None;
    }
    let cleaned = trimmed.replace(',', ".");
    cleaned.parse::<f64>().ok().and_then(|v| {
        if !v.is_finite() {
            return None;
        }
        let rounded = v.round();
        if rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
            return None;
        }
        // Range-checked against `i32` above, so the cast cannot truncate.
        #[allow(clippy::cast_possible_truncation)]
        let value = rounded as i32;
        Some(value)
    })
}

#[cfg(test)]
#[path = "text_util_tests.rs"]
mod tests;
