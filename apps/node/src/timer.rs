//! `Clock` impl backed by `tokio::time` plus a timer dispatcher.
//!
//! The orchestrator owns one [`TokioClock`] and forwards
//! `Action::ScheduleTimer` to [`schedule_event`]; when the timer fires it
//! emits `Event::TimerFired(id)` back to the SM.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use consensus::{
    event::{Event, TimerId},
    ports::clock::Clock,
};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

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

/// Tracks in-flight timer tasks so `CancelTimer` can abort them.
#[derive(Debug, Default)]
pub struct TimerRegistry {
    handles: Mutex<HashMap<TimerId, AbortHandle>>,
}

impl TimerRegistry {
    /// Abort a scheduled timer, if any.
    pub fn cancel(&self, id: TimerId) {
        if let Some(handle) = self.handles.lock().expect("timer registry lock").remove(&id) {
            handle.abort();
        }
    }
}

/// Schedule a one-shot timer. Emits `Event::TimerFired(id)` once the delay elapses.
pub fn schedule_event(
    registry: &TimerRegistry,
    out: mpsc::Sender<Event>,
    id: TimerId,
    delay_nanos: u128,
) {
    if let Some(handle) = registry
        .handles
        .lock()
        .expect("timer registry lock")
        .remove(&id)
    {
        handle.abort();
    }
    let dur = Duration::from_nanos(
        u64::try_from(delay_nanos.min(u128::from(u64::MAX))).unwrap_or(u64::MAX),
    );
    let join = tokio::spawn(async move {
        tokio::time::sleep(dur).await;
        let _ = out.send(Event::TimerFired(id)).await;
    });
    registry
        .handles
        .lock()
        .expect("timer registry lock")
        .insert(id, join.abort_handle());
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn schedule_fires_timer_event_in_order() {
        let registry = TimerRegistry::default();
        let (tx, mut rx) = mpsc::channel(4);
        schedule_event(&registry, tx.clone(), TimerId(2), 50_000_000);
        schedule_event(&registry, tx, TimerId(1), 10_000_000);
        let first = tokio::time::timeout(Duration::from_millis(200), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(first, Event::TimerFired(TimerId(1))));
        let second = tokio::time::timeout(Duration::from_millis(200), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(second, Event::TimerFired(TimerId(2))));
    }

    #[tokio::test]
    async fn cancel_prevents_timer_event() {
        let registry = TimerRegistry::default();
        let (tx, mut rx) = mpsc::channel(4);
        schedule_event(&registry, tx, TimerId(9), 20_000_000);
        registry.cancel(TimerId(9));
        let got = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(got.is_err());
    }
}
