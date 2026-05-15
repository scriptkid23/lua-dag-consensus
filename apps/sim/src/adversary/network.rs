//! Network adversary: drop / delay / duplicate / partition.

use rand::Rng;
use rand_chacha::ChaCha20Rng;

use crate::virtual_net::InFlight;

/// Network conditions applied uniformly to every message.
#[derive(Clone, Copy, Debug)]
pub struct NetworkConditions {
    /// Probability of dropping a message (0.0..=1.0).
    pub drop: f64,
    /// Maximum extra delay (ns) drawn uniformly per message.
    pub max_delay_ns: u64,
    /// Probability of duplicating (0.0..=1.0).
    pub duplicate: f64,
}

impl NetworkConditions {
    /// "Perfect network" defaults.
    #[must_use]
    pub fn perfect() -> Self {
        Self {
            drop: 0.0,
            max_delay_ns: 0,
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
