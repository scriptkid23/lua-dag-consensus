//! `Clock` impl backed by `tokio::time` plus a timer dispatcher.
//!
//! The orchestrator owns one [`TokioClock`] and forwards
//! `Action::ScheduleTimer` to [`schedule`]; when the timer fires it
//! emits `Event::TimerFired(id)` back to the SM.
//!
//! Bullshark wave timers and macro-finality timers (`T_macropropose`, Mode B
//! deadline) share this dispatcher via `Event::TimerFired`. Production gossip
//! verification for L3 topics: plan `06b-l3`.

use std::time::{Duration, Instant};

use consensus::{event::TimerId, ports::clock::Clock};
use tokio::sync::mpsc;

/// `Clock` impl that reads `Instant::now()`.
#[derive(Clone, Debug)]
pub struct TokioClock {
    started: Instant,
}

impl TokioClock {
    /// Anchor "time zero" to construction.
    #[must_use]
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }
}

impl Default for TokioClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for TokioClock {
    fn now_nanos(&self) -> u128 {
        self.started.elapsed().as_nanos()
    }
}

/// Schedule a one-shot timer. Pushes `id` onto `out` once the delay elapses.
#[allow(dead_code)]
pub fn schedule(out: mpsc::Sender<TimerId>, id: TimerId, delay_nanos: u128) {
    let dur = Duration::from_nanos(
        u64::try_from(delay_nanos.min(u128::from(u64::MAX))).unwrap_or(u64::MAX),
    );
    tokio::spawn(async move {
        tokio::time::sleep(dur).await;
        let _ = out.send(id).await;
    });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn schedule_fires_in_order() {
        let (tx, mut rx) = mpsc::channel(4);
        schedule(tx.clone(), TimerId(2), 50_000_000);
        schedule(tx, TimerId(1), 10_000_000);
        // Drain — order should reflect delay, not call order.
        let first = tokio::time::timeout(Duration::from_millis(200), rx.recv())
            .await
            .unwrap();
        assert_eq!(first.unwrap(), TimerId(1));
        let second = tokio::time::timeout(Duration::from_millis(200), rx.recv())
            .await
            .unwrap();
        assert_eq!(second.unwrap(), TimerId(2));
    }
}
