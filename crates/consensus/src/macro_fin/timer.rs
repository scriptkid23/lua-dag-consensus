//! Macro-finality timers (`T_macropropose`, Mode B deadline).

use std::collections::HashMap;

use types::primitives::Height;

use crate::{action::Action, config::Config, event::TimerId, leader::timeout::TimerScheduler};

/// Tracks macro-layer timer ids (separate namespace from bullshark wave timers).
#[derive(Debug, Default)]
pub struct MacroTimerBook {
    seq: TimerScheduler,
    backup_by_height: HashMap<u64, TimerId>,
    mode_b_by_height: HashMap<u64, TimerId>,
}

impl MacroTimerBook {
    /// Fresh timer book.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record backup-proposer timer for `height`.
    pub fn schedule_backup_propose(&mut self, height: Height, id: TimerId) {
        self.backup_by_height.insert(height.0, id);
    }

    /// Lookup backup timer id.
    #[must_use]
    pub fn backup_timer_for(&self, height: Height) -> Option<TimerId> {
        self.backup_by_height.get(&height.0).copied()
    }

    /// Drop backup timer tracking for `height`.
    pub fn clear_backup(&mut self, height: Height) {
        self.backup_by_height.remove(&height.0);
    }

    /// Record Mode B activation deadline for `height`.
    pub fn schedule_mode_b_deadline(&mut self, height: Height, id: TimerId) {
        self.mode_b_by_height.insert(height.0, id);
    }

    /// Lookup Mode B timer id.
    #[must_use]
    pub fn mode_b_timer_for(&self, height: Height) -> Option<TimerId> {
        self.mode_b_by_height.get(&height.0).copied()
    }

    /// Drop Mode B timer tracking for `height`.
    pub fn clear_mode_b(&mut self, height: Height) {
        self.mode_b_by_height.remove(&height.0);
    }

    /// Reverse lookup: backup timer id → height.
    #[must_use]
    pub fn height_for_backup_timer(&self, id: TimerId) -> Option<Height> {
        self.backup_by_height
            .iter()
            .find_map(|(h, tid)| (*tid == id).then_some(Height(*h)))
    }

    /// Reverse lookup: Mode B timer id → height.
    #[must_use]
    pub fn height_for_mode_b_timer(&self, id: TimerId) -> Option<Height> {
        self.mode_b_by_height
            .iter()
            .find_map(|(h, tid)| (*tid == id).then_some(Height(*h)))
    }

    /// Allocate a fresh timer id.
    pub fn alloc_id(&mut self) -> TimerId {
        self.seq.allocate()
    }

    /// `Action::ScheduleTimer` for backup proposer takeover.
    pub fn backup_propose_action(&mut self, cfg: &Config, height: Height) -> Action {
        let id = self.alloc_id();
        self.schedule_backup_propose(height, id);
        let delay = u128::from(cfg.timing.t_macropropose_ms) * 1_000_000;
        Action::ScheduleTimer {
            id,
            delay_nanos: delay,
        }
    }

    /// Mode B activation deadline (`2 × T_macropropose`).
    pub fn mode_b_deadline_action(&mut self, cfg: &Config, height: Height) -> Action {
        let id = self.alloc_id();
        self.schedule_mode_b_deadline(height, id);
        let delay = u128::from(cfg.timing.t_macropropose_ms) * 2_000_000;
        Action::ScheduleTimer {
            id,
            delay_nanos: delay,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::TimerId;

    #[test]
    fn backup_timer_maps_height_to_id() {
        let mut t = MacroTimerBook::new();
        let id = TimerId(42);
        t.schedule_backup_propose(Height(1), id);
        assert_eq!(t.backup_timer_for(Height(1)), Some(id));
        t.clear_backup(Height(1));
        assert_eq!(t.backup_timer_for(Height(1)), None);
    }
}
