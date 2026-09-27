//! Download event handles retained by one browser session.
//!
//! Chrome emits downloads outside the page lifecycle: a click can leave the
//! accessibility tree unchanged while a large file finishes in the background.
//! These payloads let a host observe that side effect without polling a shared
//! directory or asking a model to infer it from an unchanged page.

#[cfg(test)]
mod test;

mod types;

pub use types::{DownloadId, DownloadInfo, DownloadState, DownloadWaitRequest};
