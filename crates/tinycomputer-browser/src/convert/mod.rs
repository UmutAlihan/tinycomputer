//! Contract requests to agent-browser commands.
//!
//! agent-browser's dispatcher takes one JSON object per command, named by its
//! `action` field — the same objects its daemon receives over a socket. Every
//! function here is pure: it builds that object and nothing else, so the whole
//! mapping is testable without a browser.
//!
//! Where the contract asks for something the engine cannot express, the
//! function refuses with [`Error::InvalidInput`] naming the alternative, rather
//! than approximating it silently.

use serde_json::{Value, json};
use tinycomputer_bus::browser::{
    Action, EvaluateRequest, LocateBy, Locator, NavigateRequest, ReadFormat, ReadRequest,
    ScreenshotRequest, ScrollDirection, SessionOptions, SnapshotRequest, Target, WaitState,
    WaitUntil,
};

use crate::error::{Error, Result};

/// A distance that reaches either end of any page, for `top` and `bottom`.
const TO_THE_END: u32 = 1_000_000;

mod session;
mod page;
mod interaction;

pub(crate) use session::{launch, viewport, allowed_domains};
pub(crate) use page::{navigate, snapshot, read, evaluate, screenshot};
pub(crate) use interaction::action;

#[cfg(test)]
mod convert_tests;
