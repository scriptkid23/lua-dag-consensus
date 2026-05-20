//! Network adversary: drop / delay / duplicate / partition.

use rand::Rng;
use rand_chacha::ChaCha20Rng;

use crate::virtual_net::InFlight;

/// Network conditions applied uniformly to every message.
#[derive(Clone, Copy, Debug)]
pub struct NetworkConditions {
    /// Fixed delivery delay added before jitter (nanoseconds).
    pub base_delay_ns: u64,
    /// Maximum extra delay (ns) drawn uniformly per message.
    pub max_delay_ns: u64,
    /// Probability of dropping a message (0.0..=1.0).
    pub drop: f64,
    /// Probability of duplicating (0.0..=1.0).
    pub duplicate: f64,
}

impl NetworkConditions {
    /// "Perfect network" defaults.
    #[must_use]
    pub fn perfect() -> Self {
        Self {
            base_delay_ns: 0,
            max_delay_ns: 0,
            drop: 0.0,
            duplicate: 0.0,
        }
    }

    /// Latency-only profile derived from one micro-round duration.
    #[must_use]
    pub fn with_round_jitter(round_duration_ms: u64) -> Self {
        let round_nanos = round_duration_ms.saturating_mul(1_000_000);
        Self {
            base_delay_ns: round_nanos / 10,
            max_delay_ns: round_nanos / 5,
            drop: 0.0,
            duplicate: 0.0,
        }
    }

    /// Apply this condition to `msg`, returning `(maybe_msg, duplicate)`.
    #[must_use]
    pub fn perturb(
        &self,
        rng: &mut ChaCha20Rng,
        mut msg: InFlight,
    ) -> (Option<InFlight>, Option<InFlight>) {
        if rng.gen_range(0.0..1.0) < self.drop {
            return (None, None);
        }
        let extra = if self.max_delay_ns > 0 {
            rng.gen_range(0..=self.max_delay_ns)
        } else {
            0
        };
        msg.deliver_at += extra;
        let dup = if rng.gen_range(0.0..1.0) < self.duplicate {
            Some(msg.clone())
        } else {
            None
        };
        (Some(msg), dup)
    }
}
