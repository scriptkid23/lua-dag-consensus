//! Local side-effects from consensus `Action`s (plan 06b-l3).

use std::sync::Arc;

use consensus::{action::Action, ports::Persistence};
use storage::RocksPersistence;
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::{
    host_context::ChainedBeacon,
    observability::metrics::Metrics,
    timer::TimerRegistry,
};

/// Applies host-local actions: Rocks persistence, timers, beacon chain, blob status.
pub struct ActionApplier {
    persistence: RocksPersistence,
    timer_schedule_tx: mpsc::Sender<(consensus::event::TimerId, u128)>,
    timer_registry: Arc<TimerRegistry>,
    beacon: Arc<ChainedBeacon>,
    metrics: Arc<Metrics>,
}

impl ActionApplier {
    /// Build an applier wired to the runtime timer loop and shared beacon.
    #[must_use]
    pub fn new(
        persistence: RocksPersistence,
        timer_schedule_tx: mpsc::Sender<(consensus::event::TimerId, u128)>,
        timer_registry: Arc<TimerRegistry>,
        beacon: Arc<ChainedBeacon>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            persistence,
            timer_schedule_tx,
            timer_registry,
            beacon,
            metrics,
        }
    }

    /// Shared beacon handle (same instance as `StubHostBundle`).
    #[must_use]
    pub fn beacon(&self) -> Arc<ChainedBeacon> {
        Arc::clone(&self.beacon)
    }

    /// Apply one local action. Broadcast-only variants are no-ops here.
    pub fn apply(&mut self, action: &Action) -> anyhow::Result<()> {
        match action {
            Action::PersistMacroCheckpoint(cp) => {
                self.persistence.store_macro_checkpoint(cp)?;
            }
            Action::PersistMacroQc(qc) => {
                self.persistence.store_macro_qc(qc)?;
                self.beacon.adopt_macro_qc(qc);
            }
            Action::EmitSlashEvidence { evidence, .. } => {
                self.persistence.append_slash_evidence(evidence)?;
            }
            Action::ScheduleTimer { id, delay_nanos } => {
                let _ = self.timer_schedule_tx.try_send((*id, *delay_nanos));
            }
            Action::CancelTimer(id) => {
                self.timer_registry.cancel(*id);
            }
            Action::UpdateBlobStatus { blob, status } => {
                debug!(
                    target: "node::action_applier",
                    ?blob,
                    ?status,
                    "UpdateBlobStatus (not persisted yet)"
                );
            }
            Action::NotifyInactivityLeak {
                windows,
                bps_per_window,
            } => {
                self.metrics.inactivity_leak_emitted.inc();
                info!(
                    target: "node::action_applier",
                    windows,
                    bps_per_window,
                    "NotifyInactivityLeak"
                );
            }
            _ => {}
        }
        Ok(())
    }
}

impl std::fmt::Debug for ActionApplier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionApplier").finish_non_exhaustive()
    }
}
