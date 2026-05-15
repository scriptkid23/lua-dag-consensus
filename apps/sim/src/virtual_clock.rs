//! Virtual clock — explicit time advancement.

use std::sync::atomic::{AtomicU64, Ordering};

use consensus::ports::clock::Clock;

/// Monotonic clock with an internal nanosecond counter.
#[derive(Debug, Default)]
pub struct VirtualClock {
    nanos: AtomicU64,
}

impl VirtualClock {
    /// Construct at t=0.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance by `nanos`.
    pub fn advance(&self, nanos: u64) {
        self.nanos.fetch_add(nanos, Ordering::SeqCst);
    }

    /// Set the clock to an exact time.
    pub fn set(&self, nanos: u64) {
        self.nanos.store(nanos, Ordering::SeqCst);
    }
}

impl Clock for VirtualClock {
    fn now_nanos(&self) -> u128 {
        u128::from(self.nanos.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_is_monotonic() {
        let c = VirtualClock::new();
        c.advance(100);
        assert_eq!(c.now_nanos(), 100);
        c.advance(50);
        assert_eq!(c.now_nanos(), 150);
    }
}
