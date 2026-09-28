//! Reading the module configuration's `browser` and `cursor` keys.

use tinycomputer_browser::{CursorPace, ScreenCursor};

use crate::Result;

/// The `browser.executable` configuration: the Chrome or Chromium binary to
/// launch where the platform's own discovery would not find one.
pub(super) fn browser_executable(config: &serde_json::Value) -> Result<Option<String>> {
    let Some(browser) = config.as_object().and_then(|object| object.get("browser")) else {
        return Ok(None);
    };
    let invalid = || crate::Error::ConfigFieldType {
        field: "browser",
        expected: "an object whose optional `executable` is a string",
    };
    let browser = browser.as_object().ok_or_else(invalid)?;
    match browser.get("executable") {
        None => Ok(None),
        Some(serde_json::Value::String(path)) => Ok(Some(path.clone())),
        Some(_) => Err(invalid()),
    }
}

/// The `cursor` configuration: the agent's one on-screen cursor, shared by
/// the desktop and every task's browser. Either a pace name, or an object
/// with an optional `pace` and an optional `overlay` path to the
/// `tinycomputer-cursor-overlay` helper. Absent, the cursor glides at the
/// natural pace with the helper found where [`ProcessOverlay::locate`]
/// looks.
///
/// [`ProcessOverlay::locate`]: tinycomputer_browser::ProcessOverlay::locate
pub(super) fn cursor_config(config: &serde_json::Value) -> Result<ScreenCursor> {
    let invalid = || crate::Error::ConfigFieldType {
        field: "cursor",
        expected: "off, brisk, natural, or calm, or an object with an optional `pace` of those \
                   and an optional `overlay` path",
    };
    let pace = |value: Option<&serde_json::Value>| match value {
        None => Ok(CursorPace::default()),
        Some(serde_json::Value::String(name)) => name.parse().map_err(|_| invalid()),
        Some(_) => Err(invalid()),
    };
    let (pace, overlay) = match config.as_object().and_then(|object| object.get("cursor")) {
        None => (CursorPace::default(), None),
        Some(name @ serde_json::Value::String(_)) => (pace(Some(name))?, None),
        Some(serde_json::Value::Object(cursor)) => {
            let overlay = match cursor.get("overlay") {
                None => None,
                Some(serde_json::Value::String(path)) => Some(std::path::PathBuf::from(path)),
                Some(_) => return Err(invalid()),
            };
            (pace(cursor.get("pace"))?, overlay)
        }
        Some(_) => return Err(invalid()),
    };
    Ok(if pace.is_off() {
        ScreenCursor::off()
    } else {
        ScreenCursor::new(pace, overlay)
    })
}
