//! Drives the consensus state machine.
//!
//! Each emitted `Action` is routed by `net::gossip_wire::is_broadcast`:
//!   * broadcasts → `net_actions_tx` (consumed by the live swarm task)
//!   * local actions (timers, persistence, blob status) → the existing bridge

use std::sync::Arc;

use consensus::{StateMachine, action::Action, event::Event};
use net::Bridge;
use storage::RocksPersistence;
use tokio::sync::mpsc;
use tracing::warn;

use crate::observability::metrics::Metrics;

/// Long-running orchestrator task.
#[derive(Debug)]
pub struct Orchestrator {
    sm: StateMachine,
    bridge: Bridge,
    events_rx: mpsc::Receiver<Event>,
    metrics: Arc<Metrics>,
    /// Channel into the live gossipsub swarm; carries broadcast actions.
    net_actions_tx: mpsc::Sender<Action>,
    /// Pinned so we don't drop the storage handle prematurely.
    _persistence: RocksPersistence,
}

impl Orchestrator {
    /// Build the orchestrator. `events_rx` is the receiver counterpart
    /// of `Bridge::with_channels(events_tx, ...)`. `net_actions_tx` feeds the
    /// live swarm's outbound publish loop (see `net::swarm_runner`).
    pub fn new(
        sm: StateMachine,
        bridge: Bridge,
        events_rx: mpsc::Receiver<Event>,
        persistence: RocksPersistence,
        metrics: Arc<Metrics>,
        net_actions_tx: mpsc::Sender<Action>,
    ) -> Self {
        Self {
            sm,
            bridge,
            events_rx,
            metrics,
            net_actions_tx,
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
                        if net::gossip_wire::is_broadcast(&action) {
                            // Lossy back-pressure: if the swarm is wedged we'd
                            // rather drop one broadcast and keep consensus
                            // running than deadlock the orchestrator.
                            if let Err(e) = self.net_actions_tx.try_send(action) {
                                warn!(target: "node::orchestrator", error = %e, "net actions channel full; dropping broadcast");
                            }
                        } else if let Err(e) = Bridge::translate_action(&action) {
                            warn!(target: "node::orchestrator", error = %e, "translate action failed");
                        }
                    }
                },
                maybe_action = self.bridge.actions_rx.recv() => {
                    let Some(_action) = maybe_action else { break };
                    // BridgeHandle is unused in the live path now: broadcasts go via
                    // `net_actions_tx`. Drain anything that arrives here so the
                    // channel doesn't fill up.
                },
            }
        }
        Ok(())
    }
}
