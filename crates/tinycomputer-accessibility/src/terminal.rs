//! Terminal app detection and context extraction.

/// Normalized terminal application names (lowercase, without punctuation).
/// Extend this list to support additional terminal emulators.
pub(crate) const TERMINAL_NAMES: &[&str] = &[
    "terminal",
    "iterm2",
    "wezterm",
    "warp",
    "alacritty",
    "kitty",
    "ghostty",
    "hyper",
    "rio",
    "tabby",
    "wave",
    "contour",
    "foot",
];

#[must_use]
/// Whether an accessibility role names an editable text element.
pub fn is_text_role(role: Option<&str>) -> bool {
    matches!(
        role.unwrap_or_default(),
        "AXTextArea" | "AXTextField" | "AXSearchField" | "AXComboBox" | "AXEditableText"
    )
}

#[must_use]
/// Whether an application name looks like a terminal emulator.
pub fn is_terminal_app(app_name: Option<&str>) -> bool {
    let app: String = app_name
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect();
    TERMINAL_NAMES.iter().any(|name| app == *name)
}

#[must_use]
/// Whether captured text looks like a terminal scrollback buffer rather than prose.
pub fn looks_like_terminal_buffer(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let line_count = text.lines().count();
    line_count >= 5
        && (lower.contains("$ ")
            || lower.contains("# ")
            || lower.contains("❯")
            || lower.contains("[1] 0:")
            || lower.contains("tmux")
            || lower.contains("cargo run")
            || lower.contains("git status"))
}

fn is_terminal_noise_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return true;
    }
    trimmed.starts_with('•')
        || trimmed.starts_with('└')
        || trimmed.starts_with('─')
        || trimmed.starts_with('│')
        || (trimmed.starts_with('[')
            && (trimmed.contains(" 0:") || trimmed.contains("[tmux]") || trimmed.contains("\"⠙")))
}

#[must_use]
/// Extract the text around the current prompt from a terminal buffer.
pub fn extract_terminal_input_context(text: &str) -> String {
    let mut fallback = String::new();
    for raw_line in text.lines().rev().take(40) {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if fallback.is_empty() && !is_terminal_noise_line(line) {
            fallback = line.to_string();
        }
        if is_terminal_noise_line(line) {
            continue;
        }
        if line.contains("$ ")
            || line.contains("# ")
            || line.contains("❯")
            || line.contains("➜")
            || line.contains("λ")
        {
            return line.to_string();
        }
    }
    fallback
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
