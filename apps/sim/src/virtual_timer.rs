//! Deterministic timer queue for the simulator.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use consensus::event::TimerId;

/// Min-heap timer queue keyed by `deliver_at`.
#[derive(Debug, Default)]
pub struct VirtualTimer {
    pending: BinaryHeap<Reverse<(u64, u64)>>,
    cancelled: std::collections::HashSet<u64>,
}

impl VirtualTimer {
    /// Empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedule `id` to fire at `deliver_at`.
    pub fn schedule(&mut self, deliver_at: u64, id: TimerId) {
        self.cancelled.remove(&id.0);
        self.pending.push(Reverse((deliver_at, id.0)));
    }

    /// Cancel a pending timer.
    pub fn cancel(&mut self, id: TimerId) {
        self.cancelled.insert(id.0);
    }

    /// Pop all timers with `deliver_at <= now`.
    pub fn drain_due(&mut self, now: u64) -> Vec<TimerId> {
        let mut out = Vec::new();
        while let Some(Reverse((at, _))) = self.pending.peek().copied() {
            if at > now {
                break;
            }
            let Reverse((_, id)) = self.pending.pop().unwrap();
            if self.cancelled.remove(&id) {
                continue;
            }
            out.push(TimerId(id));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_due_respects_order() {
        let mut t = VirtualTimer::new();
        t.schedule(30, TimerId(2));
        t.schedule(10, TimerId(0));
        t.schedule(20, TimerId(1));
        let due = t.drain_due(25);
        assert_eq!(due, vec![TimerId(0), TimerId(1)]);
        assert_eq!(t.drain_due(100), vec![TimerId(2)]);
    }
}
