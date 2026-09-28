//! Tests for the surface-neutral helpers: fingerprints, change notes, names,
//! and verified text delivery over a scripted surface.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use serde_json::json;
use tinycomputer_bus::{DesktopResponse, JevOperation};

use super::{
    Candidate, Depth, Screen, Surface, change_note, deliver_text, describe, element_line,
    exact_named_match, fingerprint, holds, result_families, result_groups, target_payload,
    tokenized, uses_pointer,
};

mod delivery_tests;
mod groups_tests;
mod screen_tests;
