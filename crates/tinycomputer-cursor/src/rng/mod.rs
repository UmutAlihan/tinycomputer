//! A small seeded generator (`SplitMix64`).
//!
//! Motion only needs variety, not cryptographic quality, and a seed makes a
//! gesture reproducible: tests pin one, and a trace can record one so a run
//! replays the same path.

/// A seeded pseudo-random generator.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// A generator that always produces the same sequence for `seed`.
    #[must_use]
    pub const fn seeded(seed: u64) -> Self {
        Self { state: seed }
    }

    /// A generator seeded from the operating system, so two runs differ.
    ///
    /// Falls back to a fixed seed when the system has no entropy to give,
    /// which costs variety, never correctness.
    #[must_use]
    pub fn from_entropy() -> Self {
        let mut bytes = [0_u8; 8];
        if getrandom::fill(&mut bytes).is_err() {
            return Self::seeded(0x5EED_CAFE_F00D_D00D);
        }
        Self::seeded(u64::from_le_bytes(bytes))
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A uniform number in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        let high = u32::try_from(self.next_u64() >> 32).unwrap_or(u32::MAX);
        f64::from(high) / 4_294_967_296.0
    }

    /// A uniform number in `[low, high)`.
    pub fn range(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.unit()
    }

    /// A normally distributed number (Box–Muller).
    pub fn normal(&mut self, mean: f64, deviation: f64) -> f64 {
        let u1 = 1.0 - self.unit();
        let u2 = self.unit();
        mean + deviation * (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }

    /// `true` with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }
}

impl Default for Rng {
    fn default() -> Self {
        Self::from_entropy()
    }
}

#[cfg(test)]
mod rng_tests;
