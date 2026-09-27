//! Request payloads for the pasteboard members.
//!
//! The pasteboard is shared with whoever is using the machine, so these
//! members are the one place where a headless run unavoidably touches
//! something a person can see. Read what is there with
//! [`ClipboardGetRequest`], put something there with [`ClipboardSetRequest`],
//! and empty it with `ClipboardClear`, which takes no argument.

mod types;

pub use types::{ClipboardGetRequest, ClipboardSetRequest};

#[cfg(test)]
mod test;
