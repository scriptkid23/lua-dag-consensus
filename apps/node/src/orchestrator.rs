//! Drives the consensus state machine.

use std::sync::Arc;

use consensus::{StateMachine, event::Event};
use net::Bridge;
use storage::RocksPersistence;
use tokio::sync::mpsc;
use tracing::warn;

use crate::observability::metrics::Metrics;

/// Long-running orchestrator task.
pub struct Orchestrator {
    sm: StateMachine,
    bridge: Bridge,
    events_rx: mpsc::Receiver<Event>,
    metrics: Arc<Metrics>,
    /// Pinned so we don't drop the storage handle prematurely.
    _persistence: RocksPersistence,
}

impl Orchestrator {
    /// Build the orchestrator. `events_rx` is the receiver counterpart
    /// of `Bridge::with_channels(events_tx, ...)`.
    pub fn new(
        sm: StateMachine,
        bridge: Bridge,
        events_rx: mpsc::Receiver<Event>,
        persistence: RocksPersistence,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            sm,
            bridge,
            events_rx,
            metrics,
            _persistence: persistence,
        }
    }

    /// Main loop. Returns when `events_rx` is closed.
    pub async fn run(mut self) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                maybe_event = self.events_rx.recv() => {
                    let Some(event) = maybe_event else { break };
                    self.metrics.events_processed.inc();
                    let actions = match self.sm.step(event) {
                        Ok(a) => a,
                        Err(e) => {
                            warn!(target: "node::orchestrator", error = %e, "consensus step failed");
                            continue;
                        }
                    };
                    for action in actions {
                        self.metrics.actions_dispatched.inc();
                        if let Err(e) = Bridge::translate_action(&action) {
                            warn!(target: "node::orchestrator", error = %e, "translate action failed");
                        }
                    }
                },
                maybe_action = self.bridge.actions_rx.recv() => {
                    let Some(_action) = maybe_action else { break };
                    // TODO(plan 06+): actually publish via libp2p swarm.
                },
            }
        }
        Ok(())
    }
}
