//! Tests for reading sight's reply: controls, text, and when the tree is
//! read instead.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::{Denoised, denoised, is_seen, screen, script, selector};

mod live_tests;
mod reading_tests;
