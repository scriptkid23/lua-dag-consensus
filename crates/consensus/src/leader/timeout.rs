//! Central timer scheduler. The SM emits `Action::ScheduleTimer` /
//! `Action::CancelTimer`; the host translates those into real timers.

use crate::event::TimerId;

/// Allocates monotonic `TimerId`s for the SM. Stays internal — host
/// binaries never call this directly.
#[derive(Debug, Default)]
pub struct TimerScheduler {
    next: u64,
}

impl TimerScheduler {
    /// Allocate a new `TimerId`.
    pub fn allocate(&mut self) -> TimerId {
        let id = TimerId(self.next);
        self.next += 1;
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_monotonic_ids() {
        let mut s = TimerScheduler::default();
        assert_eq!(s.allocate(), TimerId(0));
        assert_eq!(s.allocate(), TimerId(1));
    }
}
