//! Logical keys, and the combination each one is on each platform.
//!
//! A flow says "select all" or "go back", never `cmd+a`: the same step must
//! work on macOS, Windows, and Linux, and in a desktop application or a web
//! page. [`Key::desktop`] spells a key for agent-desktop's `Press` member
//! (`cmd+a`), and [`Key::browser`] for agent-browser's `press` command
//! (`Meta+a`).

/// The operating system whose conventions a key follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// macOS: the command key.
    MacOs,
    /// Windows: the control key.
    Windows,
    /// Linux: the control key.
    Linux,
}

impl Platform {
    /// The platform this build targets.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// A key or shortcut named by what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Select everything in the focused field or document.
    SelectAll,
    /// Copy the selection.
    Copy,
    /// Cut the selection.
    Cut,
    /// Paste the clipboard.
    Paste,
    /// Undo the last change.
    Undo,
    /// Redo the last undone change.
    Redo,
    /// Create a new document, message, or item.
    New,
    /// Open search or find.
    Find,
    /// Go back.
    Back,
    /// Go forward.
    Forward,
    /// Reload the view.
    Refresh,
    /// Open the application's settings.
    Settings,
    /// Confirm: the Return or Enter key.
    Confirm,
    /// Dismiss: the Escape key.
    Dismiss,
    /// Move to the next field.
    NextField,
}

impl Key {
    /// The combination for agent-desktop's `Press` member, such as `cmd+a`.
    ///
    /// `None` when the platform has no standard shortcut for the key, as
    /// Windows and Linux have none for settings.
    #[must_use]
    pub fn desktop(self, platform: Platform) -> Option<String> {
        let command = if platform == Platform::MacOs {
            "cmd"
        } else {
            "ctrl"
        };
        let mac = platform == Platform::MacOs;
        Some(match self {
            Self::SelectAll => format!("{command}+a"),
            Self::Copy => format!("{command}+c"),
            Self::Cut => format!("{command}+x"),
            Self::Paste => format!("{command}+v"),
            Self::Undo => format!("{command}+z"),
            Self::Redo if mac => "cmd+shift+z".to_owned(),
            Self::Redo => "ctrl+y".to_owned(),
            Self::New => format!("{command}+n"),
            Self::Find => format!("{command}+f"),
            Self::Back if mac => "cmd+[".to_owned(),
            Self::Back => "alt+left".to_owned(),
            Self::Forward if mac => "cmd+]".to_owned(),
            Self::Forward => "alt+right".to_owned(),
            Self::Refresh => format!("{command}+r"),
            Self::Settings if mac => "cmd+,".to_owned(),
            Self::Settings => return None,
            Self::Confirm => "return".to_owned(),
            Self::Dismiss => "escape".to_owned(),
            Self::NextField => "tab".to_owned(),
        })
    }

    /// The key for agent-browser's `press` command, such as `Meta+a`.
    ///
    /// `None` for keys a web page has no use for, such as settings.
    #[must_use]
    pub fn browser(self, platform: Platform) -> Option<String> {
        let command = if platform == Platform::MacOs {
            "Meta"
        } else {
            "Control"
        };
        Some(match self {
            Self::SelectAll => format!("{command}+a"),
            Self::Copy => format!("{command}+c"),
            Self::Cut => format!("{command}+x"),
            Self::Paste => format!("{command}+v"),
            Self::Undo => format!("{command}+z"),
            Self::Redo => format!("{command}+Shift+z"),
            Self::Find => format!("{command}+f"),
            Self::Back => "Alt+ArrowLeft".to_owned(),
            Self::Forward => "Alt+ArrowRight".to_owned(),
            Self::Refresh => "F5".to_owned(),
            Self::Confirm => "Enter".to_owned(),
            Self::Dismiss => "Escape".to_owned(),
            Self::NextField => "Tab".to_owned(),
            Self::New | Self::Settings => return None,
        })
    }
}

#[cfg(test)]
mod keymap_tests;
