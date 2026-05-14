//! `RandomnessBeacon` port. Returns the latest beacon output. Chaining
//! itself is computed inside `consensus::leader::beacon`.

use types::crypto_types::Hash32;

use crate::error::Result;

/// Provider of randomness-beacon outputs.
pub trait RandomnessBeacon: Send + Sync {
    /// Return the latest beacon output (`R_w` for the current window).
    fn current(&self) -> Result<Hash32>;
}
