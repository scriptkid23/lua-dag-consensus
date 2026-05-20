//! In-memory deterministic message bus.

use std::collections::{HashSet, VecDeque};

use consensus::{action::Action, event::Event};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

use crate::adversary::network::NetworkConditions;

/// Two-partition network split; cross-partition delivery blocked while active.
#[derive(Clone, Debug, Default)]
pub struct Partition {
    /// Validators on the left side.
    left: HashSet<u32>,
    /// Validators on the right side.
    right: HashSet<u32>,
    /// When false, all pairs may communicate.
    active: bool,
}

impl Partition {
    /// Build an active partition from two disjoint sides.
    #[must_use]
    pub fn new(left: impl IntoIterator<Item = u32>, right: impl IntoIterator<Item = u32>) -> Self {
        Self {
            left: left.into_iter().collect(),
            right: right.into_iter().collect(),
            active: true,
        }
    }

    /// Resume cross-partition delivery.
    pub fn heal(&mut self) {
        self.active = false;
    }

    /// True when `active` is set.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Whether a message from `sender` may be delivered to `recipient`.
    #[must_use]
    pub fn allows(&self, sender: u32, recipient: u32) -> bool {
        if !self.active {
            return true;
        }
        (self.left.contains(&sender) && self.left.contains(&recipient))
            || (self.right.contains(&sender) && self.right.contains(&recipient))
    }
}

/// One in-flight network message.
#[derive(Clone, Debug)]
pub struct InFlight {
    /// Validator index that should receive this event.
    pub recipient: u32,
    /// Wrapped event.
    pub event: Event,
    /// Virtual time at which delivery becomes eligible.
    pub deliver_at: u64,
}

/// Deterministic message queue.
#[derive(Debug)]
pub struct VirtualNet {
    /// Pending messages, sorted by `deliver_at`.
    pending: VecDeque<InFlight>,
    /// Latency / drop / duplicate policy.
    conditions: NetworkConditions,
    /// Optional partition split.
    partition: Option<Partition>,
}

impl Default for VirtualNet {
    fn default() -> Self {
        Self {
            pending: VecDeque::new(),
            conditions: NetworkConditions::perfect(),
            partition: None,
        }
    }
}

impl VirtualNet {
    /// New empty bus with perfect delivery.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of in-flight messages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// True if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Active network conditions (latency, drop, duplicate).
    #[must_use]
    pub fn conditions(&self) -> NetworkConditions {
        self.conditions
    }

    /// Replace network conditions.
    pub fn set_conditions(&mut self, conditions: NetworkConditions) {
        self.conditions = conditions;
    }

    /// Install a two-way partition.
    pub fn set_partition(
        &mut self,
        left: impl IntoIterator<Item = u32>,
        right: impl IntoIterator<Item = u32>,
    ) {
        self.partition = Some(Partition::new(left, right));
    }

    /// Heal the active partition, if any.
    pub fn heal_partition(&mut self) {
        if let Some(p) = &mut self.partition {
            p.heal();
        }
    }

    /// True when a partition is installed and still active.
    #[must_use]
    pub fn partition_active(&self) -> bool {
        self.partition.as_ref().is_some_and(Partition::is_active)
    }

    /// Translate an outbound `Action` from `sender` into events for every
    /// recipient that should receive it.
    pub fn enqueue_from_action(
        &mut self,
        sender: u32,
        action: &Action,
        validator_count: u32,
        now: u64,
        rng: &mut ChaCha20Rng,
    ) {
        if let Action::BroadcastMicroQc(qc) = action {
            for recipient in 0..validator_count {
                if recipient == sender {
                    continue;
                }
                if !self.allows_delivery(sender, recipient) {
                    continue;
                }
                let deliver_at = now
                    .saturating_add(self.conditions.base_delay_ns)
                    .saturating_add(jitter_nanos(rng, self.conditions.max_delay_ns));
                let msg = InFlight {
                    recipient,
                    event: Event::MicroQcAssembled(qc.clone()),
                    deliver_at,
                };
                let (maybe, duplicate) = self.conditions.perturb(rng, msg);
                if let Some(m) = maybe {
                    self.enqueue(m);
                }
                if let Some(d) = duplicate {
                    self.enqueue(d);
                }
            }
        }
    }

    fn allows_delivery(&self, sender: u32, recipient: u32) -> bool {
        self.partition
            .as_ref()
            .is_none_or(|p| p.allows(sender, recipient))
    }

    /// Push a raw message (used directly by adversaries).
    pub fn enqueue(&mut self, msg: InFlight) {
        let pos = self
            .pending
            .partition_point(|m| m.deliver_at <= msg.deliver_at);
        self.pending.insert(pos, msg);
    }

    /// Pop all messages with `deliver_at <= now` in FIFO order within
    /// each timestamp; deterministic because of stable sort + insertion.
    pub fn drain_due(&mut self, now: u64) -> Vec<InFlight> {
        let mut out = Vec::new();
        while let Some(front) = self.pending.front() {
            if front.deliver_at <= now {
                out.push(self.pending.pop_front().unwrap());
            } else {
                break;
            }
        }
        out
    }
}

/// Helper: inject network jitter using the provided RNG, returning a
/// delay in nanoseconds. Determinism comes from the caller's RNG.
pub fn jitter_nanos(rng: &mut ChaCha20Rng, max: u64) -> u64 {
    if max == 0 { 0 } else { rng.gen_range(0..=max) }
}

#[cfg(test)]
mod tests {
    use consensus::event::TimerId;
    use rand::SeedableRng;
    use types::{
        crypto_types::{BlsAggSig, BlsSig, Hash32},
        micro::MicroQc,
    };

    use super::*;

    #[test]
    fn drain_due_returns_only_ready_messages() {
        let mut net = VirtualNet::new();
        net.enqueue(InFlight {
            recipient: 0,
            event: Event::TimerFired(TimerId(0)),
            deliver_at: 10,
        });
        net.enqueue(InFlight {
            recipient: 1,
            event: Event::TimerFired(TimerId(1)),
            deliver_at: 30,
        });
        let due_at_20 = net.drain_due(20);
        assert_eq!(due_at_20.len(), 1);
        assert_eq!(due_at_20[0].recipient, 0);
        assert_eq!(net.len(), 1);
    }

    #[test]
    fn jitter_is_deterministic_for_given_seed() {
        let mut a = ChaCha20Rng::from_seed([7; 32]);
        let mut b = ChaCha20Rng::from_seed([7; 32]);
        assert_eq!(jitter_nanos(&mut a, 100), jitter_nanos(&mut b, 100));
    }

    #[test]
    fn partition_blocks_cross_side_micro_qc() {
        let mut net = VirtualNet::new();
        net.set_partition([0, 1], [2, 3]);
        let mut rng = ChaCha20Rng::from_seed([1; 32]);
        let qc = MicroQc {
            checkpoint_hash: Hash32([0xAB; 32]),
            agg: BlsAggSig {
                sig: BlsSig([0; 96]),
                bitmap: vec![0xFF],
            },
        };
        net.enqueue_from_action(0, &Action::BroadcastMicroQc(qc), 4, 0, &mut rng);
        assert_eq!(net.len(), 1);
        assert_eq!(net.pending[0].recipient, 1);
    }

    #[test]
    fn heal_restores_cross_partition_delivery() {
        let mut net = VirtualNet::new();
        net.set_partition([0], [1]);
        let mut p = Partition::new([0], [1]);
        assert!(!p.allows(0, 1));
        p.heal();
        assert!(p.allows(0, 1));
        net.heal_partition();
        assert!(!net.partition_active());
    }
}
