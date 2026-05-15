//! Static randomness beacon used by the simulator. Real chaining can
//! be tested separately via `consensus::leader::beacon::chain_beacon`.

use std::sync::RwLock;

use consensus::ports::rng_beacon::RandomnessBeacon;
use types::crypto_types::Hash32;

/// Mutable beacon container.
#[derive(Debug, Default)]
pub struct VirtualBeacon {
    current: RwLock<Hash32>,
}

impl VirtualBeacon {
    /// Construct with initial value.
    #[must_use]
    pub fn new(initial: Hash32) -> Self {
        Self {
            current: RwLock::new(initial),
        }
    }

    /// Replace the current value.
    pub fn set(&self, h: Hash32) {
        *self.current.write().unwrap() = h;
    }
}

impl RandomnessBeacon for VirtualBeacon {
    fn current(&self) -> consensus::Result<Hash32> {
        Ok(*self.current.read().unwrap())
    }
}
