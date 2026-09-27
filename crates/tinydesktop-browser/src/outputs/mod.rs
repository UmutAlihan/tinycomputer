//! The held outputs, and the bounds on them.
//!
//! # What this is defending against
//!
//! An output is a screenshot sitting in the module's memory waiting to be
//! collected, and the module is loaded into somebody else's process. Three
//! things can go wrong there, and all three are ordinary rather than hostile: a
//! host takes a screenshot and never reads it, a host takes hundreds in a loop,
//! or one screenshot of a very long page is larger than anybody expected.
//!
//! So the store caps the number of live outputs, caps the size of each, and
//! expires anything left uncollected. Eviction is oldest-first, which is the
//! right order when the alternative is refusing the newest: the screenshot just
//! taken is the one a caller is about to read.

use std::collections::HashMap;
use std::fmt::Write;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use sha2::{Digest, Sha256};
use tinybrowser_bus::{OutputChunk, OutputId, OutputRef};

use crate::error::{Error, Result};

/// How many outputs may be held at once.
const MAX_OUTPUTS: usize = 16;

/// The largest single output this module will hold.
///
/// A full-page capture of a long article at 2x lands in the low megabytes; this
/// is several times that and still far below what would matter to a host.
pub(crate) const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

/// How long an uncollected output survives.
pub(crate) const TTL: Duration = Duration::from_secs(300);

/// How often the sweeper looks for outputs to drop.
///
/// A fraction of [`TTL`], so an abandoned output is released within a minute or
/// so of expiring rather than at some unbounded later moment.
pub(crate) const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

/// The most a single [`read`](OutputStore::read) will return.
///
/// Below the bus frame limit with room for the base64 expansion — four bytes out
/// for every three in — and the JSON envelope around it.
const MAX_CHUNK: u64 = 4 * 1024 * 1024;

/// One held output.
///
/// Only the bytes and when they arrived: the size, digest, and dimensions went
/// out on the [`OutputRef`] when the output was stored, and keeping a second
/// copy here would be two places for them to disagree.
#[derive(Debug)]
struct Held {
    bytes: Vec<u8>,
    stored: Instant,
}

/// Outputs waiting to be collected.
#[derive(Debug, Default)]
pub(crate) struct OutputStore {
    held: HashMap<OutputId, Held>,
}

impl OutputStore {
    /// Holds `bytes` and returns the handle a host collects them with.
    ///
    /// # Errors
    ///
    /// [`Error::LimitExceeded`] when the output is larger than this module will
    /// hold.
    pub(crate) fn insert(
        &mut self,
        bytes: Vec<u8>,
        media_type: &str,
        width: u32,
        height: u32,
    ) -> Result<OutputRef> {
        if bytes.len() > MAX_OUTPUT_BYTES {
            return Err(Error::LimitExceeded {
                message: format!(
                    "output of {} bytes exceeds the {MAX_OUTPUT_BYTES} byte cap",
                    bytes.len()
                ),
            });
        }

        self.expire();
        while self.held.len() >= MAX_OUTPUTS {
            let Some(oldest) = self
                .held
                .iter()
                .min_by_key(|(_, held)| held.stored)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            self.held.remove(&oldest);
        }

        let digest = Sha256::digest(&bytes);
        let mut sha256 = String::with_capacity(digest.len() * 2);
        for byte in digest.as_slice() {
            let _ = write!(sha256, "{byte:02x}");
        }
        let id = OutputId::new(uuid::Uuid::new_v4().to_string());
        let handle = OutputRef {
            id: id.clone(),
            total_bytes: bytes.len() as u64,
            sha256: sha256.clone(),
            media_type: media_type.to_string(),
            width,
            height,
        };

        self.held.insert(
            id,
            Held {
                bytes,
                stored: Instant::now(),
            },
        );

        Ok(handle)
    }

    /// Reads up to `len` bytes of `id` from `offset`.
    ///
    /// # Errors
    ///
    /// [`Error::NoSuchOutput`] when the output is unknown or has expired, and
    /// [`Error::InvalidInput`] when `offset` is past its end.
    pub(crate) fn read(&mut self, id: &OutputId, offset: u64, len: u64) -> Result<OutputChunk> {
        self.expire();

        let held = self
            .held
            .get(id)
            .ok_or_else(|| Error::NoSuchOutput { id: id.to_string() })?;

        let total = held.bytes.len() as u64;
        if offset > total {
            return Err(Error::invalid_input(format!(
                "offset {offset} is past the end of a {total} byte output"
            )));
        }

        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(held.bytes.len());
        let take = usize::try_from(len.clamp(1, MAX_CHUNK)).unwrap_or(0);
        let end = start.saturating_add(take).min(held.bytes.len());

        Ok(OutputChunk {
            id: id.clone(),
            offset,
            data: BASE64.encode(&held.bytes[start..end]),
            eof: end >= held.bytes.len(),
        })
    }

    /// Drops `id`, whether or not it was there.
    ///
    /// Releasing something already gone is the outcome being asked for, so this
    /// cannot fail: a host retrying a release must not have to tell "never
    /// existed" apart from "already cleaned up".
    pub(crate) fn release(&mut self, id: &OutputId) {
        self.held.remove(id);
    }

    /// Whether an output is still held.
    #[cfg(test)]
    pub(crate) fn holds(&self, id: &OutputId) -> bool {
        self.held.contains_key(id)
    }

    /// How many outputs are held.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.held.len()
    }

    /// Drops everything past its time to live.
    ///
    /// Called from the operations *and* from a sweeper, because an expiry that
    /// only runs when something else happens is not an expiry: a host that takes
    /// sixteen large screenshots and then goes quiet would hold every byte of
    /// them, in somebody else's process, until it happened to call again.
    pub(crate) fn expire(&mut self) {
        let now = Instant::now();
        self.held
            .retain(|_, held| now.duration_since(held.stored) < TTL);
    }
}

#[cfg(test)]
impl OutputStore {
    /// Ages every held output by `elapsed`, as if that much time had passed.
    ///
    /// Expiry is the one behaviour here that is a function of the clock, and a
    /// test that waited five real minutes to check it would never be run. Moving
    /// the timestamps back instead keeps the assertion exact and instant.
    pub(crate) fn age(&mut self, elapsed: Duration) {
        for held in self.held.values_mut() {
            held.stored -= elapsed;
        }
    }
}
