//! Runnable examples and opt-in live verification for tinydesktop.
//!
//! The binaries in `src/bin` keep demonstration and release-verification code
//! out of the loadable module crate while remaining compiled by workspace CI.
//!
//! ```
//! use tinydesktop_bus::{JevProvider, RunGoalRequest};
//!
//! let request = RunGoalRequest {
//!     app: "Spotify".to_owned(),
//!     goal: "play the topmost liked song".to_owned(),
//!     ..RunGoalRequest::default()
//! };
//! assert_eq!(request.app, "Spotify");
//! assert_eq!(JevProvider::OpenRouter, JevProvider::OpenRouter);
//! ```

pub mod journal;
pub mod lab;
