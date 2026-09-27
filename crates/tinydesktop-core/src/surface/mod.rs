//! The surface abstraction every decision loop runs against.
//!
//! A [`Surface`] observes a [`Screen`] of [`Candidate`]s and acts on them with
//! closed operations. `tinydesktop-desktop` implements it over agent-desktop
//! and `tinydesktop-browser` over agent-browser; the engine's flow runtime is
//! generic over it. The helpers here — fingerprints, change notes, verified
//! text delivery — depend on nothing but the trait.

mod delivery;
mod screen;

use tinydesktop_bus::{DesktopResponse, JevOperation};

pub use delivery::{deliver_text, holds, tokenized};
pub use screen::{
    Candidate, Depth, MAX_CANDIDATES, Screen, change_note, describe, difference, exact_named_match,
    fingerprint, label, signature, target_payload, untrusted_context,
};

/// One thing a task can observe and act on: a desktop application or a
/// browser tab.
///
/// The flow runtime is written against this trait alone, so its decision
/// loops run the same over agent-desktop and agent-browser, and a test drives
/// them with a scripted implementation.
pub trait Surface: Clone + Send + 'static {
    /// Reads the current surface of `app`, optionally rooted at a container.
    fn observe(
        &self,
        app: &str,
        root: Option<&str>,
        depth: Depth,
    ) -> Result<Screen, Box<DesktopResponse>>;

    /// Runs one closed operation. `TypeText` only sets the value; callers that
    /// need it verified go through [`deliver_text`].
    fn execute(
        &self,
        operation: JevOperation,
        target: Option<Candidate>,
        text: Option<String>,
    ) -> DesktopResponse;

    /// Reads an element's current value, when the platform exposes one.
    fn read_value(&self, target: &Candidate) -> Option<String>;

    /// Focuses `target`, puts `text` into it through the pasteboard, and
    /// restores whatever the pasteboard held before.
    ///
    /// A field that supports set-value is replaced (select all, then paste).
    /// One that does not — a rich-text body such as a mail message — has the
    /// text pasted at the caret, so a reply keeps the message it quotes.
    fn paste(&self, app: &str, target: &Candidate, text: &str) -> DesktopResponse;

    /// Presses a key combination at `app`.
    fn press(&self, app: &str, combo: &str) -> DesktopResponse;

    /// Launches `app`, or brings it forward when it is already running.
    fn launch(&self, app: &str) -> DesktopResponse;

    /// Gives the application a moment to commit what it was just given, as a
    /// token field does when it turns an address into a token.
    fn settle(&self) {}
}

#[cfg(test)]
mod test;
