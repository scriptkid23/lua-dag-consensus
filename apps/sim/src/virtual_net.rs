//! In-memory deterministic message bus.

use std::collections::VecDeque;

use consensus::{action::Action, event::Event};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

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
#[derive(Debug, Default)]
pub struct VirtualNet {
    /// Pending messages, sorted by `deliver_at`.
    pending: VecDeque<InFlight>,
}

impl VirtualNet {
    /// New empty bus.
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

    /// Translate an outbound `Action` from `sender` into events for every
    /// recipient that should receive it. Skeleton: drops every action
    /// type (mirrors `net::bridge`'s skeleton). Plan 03b+ extends this.
    #[allow(clippy::unused_self)] // skeleton will use `self` when mapping broadcasts
    pub fn enqueue_from_action(
        &mut self,
        _sender: u32,
        _action: &Action,
        _validator_count: u32,
        _now: u64,
        _rng: &mut ChaCha20Rng,
    ) {
        // Skeleton no-op. When consensus algorithms emit actions, this
        // method maps broadcasts to per-validator deliveries.
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
    rng.gen_range(0..=max)
}

#[cfg(test)]
mod tests {
    use consensus::event::TimerId;
    use rand::SeedableRng;

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
}
