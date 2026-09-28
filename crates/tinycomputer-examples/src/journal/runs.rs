//! Finding journaled runs on disk and reading their events.

use std::path::{Path, PathBuf};

use serde_json::Value;
use tinycomputer_engine::{JOURNAL_DEFAULT_DIR, JOURNAL_ENV, JOURNAL_FILE};

/// The directory runs are journaled under: the value of the journal
/// environment variable when it names a directory, the default otherwise.
#[must_use]
pub fn root() -> PathBuf {
    match std::env::var(JOURNAL_ENV) {
        Ok(value)
            if !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "" | "0" | "1" | "true" | "false" | "on" | "off" | "yes" | "no"
            ) =>
        {
            PathBuf::from(value)
        }
        _ => PathBuf::from(JOURNAL_DEFAULT_DIR),
    }
}

/// The run directories under `root` that hold a journal, oldest first. Run
/// ids start with their start time, so name order is time order; task
/// journals (`task-…`) sort after them.
///
/// # Errors
///
/// Returns the I/O error when `root` cannot be read.
pub fn runs(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut runs = std::fs::read_dir(root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join(JOURNAL_FILE).is_file())
        .collect::<Vec<_>>();
    runs.sort();
    Ok(runs)
}

/// Finds a run under `root` by `name`: `latest`, a whole run id, or the
/// unique run whose id contains `name`. A path to a run directory is used as
/// it is.
///
/// # Errors
///
/// Returns a `NotFound` error when nothing matches, or more than one run does.
pub fn find(root: &Path, name: &str) -> std::io::Result<PathBuf> {
    let direct = PathBuf::from(name);
    if direct.join(JOURNAL_FILE).is_file() {
        return Ok(direct);
    }
    let runs = runs(root)?;
    let matches = if name == "latest" {
        runs.last().cloned().into_iter().collect::<Vec<_>>()
    } else {
        runs.into_iter()
            .filter(|run| {
                run.file_name()
                    .is_some_and(|id| id.to_string_lossy().contains(name))
            })
            .collect()
    };
    match matches.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(not_found(format!("no journaled run matches {name:?}"))),
        many => Err(not_found(format!(
            "{} journaled runs match {name:?}; give more of the id",
            many.len()
        ))),
    }
}

/// Every event in the run at `dir`. A line that does not parse — the last
/// one of a run still being written, say — is skipped.
///
/// # Errors
///
/// Returns the I/O error when the journal cannot be read.
pub fn events(dir: &Path) -> std::io::Result<Vec<Value>> {
    Ok(std::fs::read_to_string(dir.join(JOURNAL_FILE))?
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect())
}

fn not_found(message: String) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::NotFound, message)
}
