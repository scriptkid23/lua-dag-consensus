//! Drives the consensus state machine.
//!
//! Each emitted `Action` is routed by `net::gossip_wire::is_broadcast`:
//!   * broadcasts → `net_actions_tx` (consumed by the live swarm task)
//!   * local actions → [`ActionApplier`] (persistence, timers, beacon)

use std::sync::Arc;

use consensus::{StateMachine, action::Action, event::Event};
use net::Bridge;
use storage::RocksPersistence;
use tokio::sync::mpsc;
use tracing::warn;
use types::validator::ValidatorSet;

use crate::{
    action_applier::ActionApplier,
    host_context::StubHostBundle,
    observability::metrics::Metrics,
};

/// Long-running orchestrator task.
#[derive(Debug)]
pub struct Orchestrator {
    sm: StateMachine,
    bridge: Bridge,
    events_rx: mpsc::Receiver<Event>,
    metrics: Arc<Metrics>,
    /// Channel into the live gossipsub swarm; carries broadcast actions.
    net_actions_tx: mpsc::Sender<Action>,
    /// Host port bundle for `StateMachine::step`.
    host_bundle: StubHostBundle,
    /// Rocks-backed persistence for `HostContext` and local actions.
    persistence: RocksPersistence,
    /// Local side-effects (persist, timers, beacon).
    action_applier: ActionApplier,
    valset: ValidatorSet,
    l1_real_vertex_certs: bool,
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
        host_bundle: StubHostBundle,
        action_applier: ActionApplier,
        valset: ValidatorSet,
        l1_real_vertex_certs: bool,
    ) -> Self {
        Self {
            sm,
            bridge,
            events_rx,
            metrics,
            net_actions_tx,
            host_bundle,
            persistence,
            action_applier,
            valset,
            l1_real_vertex_certs,
        }
    }

    /// Main loop. Returns when `events_rx` is closed.
    pub async fn run(mut self) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                maybe_event = self.events_rx.recv() => {
                    let Some(event) = maybe_event else { break };
                    self.metrics.events_processed.inc();
                    if let Event::CertifiedVertexReceived(cv) = &event {
                        if self.l1_real_vertex_certs {
                            if let Err(e) = dag::cert::verify_certified_vertex(cv, &self.valset) {
                                warn!(
                                    target: "node::orchestrator",
                                    error = %e,
                                    "rejecting certified vertex"
                                );
                                self.metrics.vertex_cert_rejected.inc();
                                continue;
                            }
                        }
                        if let Err(e) = self.host_bundle.dag.ingest(cv.clone()) {
                            warn!(
                                target: "node::orchestrator",
                                error = %e,
                                "failed to ingest certified vertex"
                            );
                            continue;
                        }
                    }
                    let ctx = crate::host_context::build_host_context(
                        &self.host_bundle,
                        &self.persistence,
                    );
                    let actions = match self.sm.step(event, &ctx) {
                        Ok(a) => a,
                        Err(e) => {
                            warn!(target: "node::orchestrator", error = %e, "consensus step failed");
                            continue;
                        }
                    };
                    for action in actions {
                        self.metrics.actions_dispatched.inc();
                        if net::gossip_wire::is_broadcast(&action) {
                            if let Err(e) = self.net_actions_tx.try_send(action.clone()) {
                                self.metrics.actions_dropped.inc();
                                warn!(target: "node::orchestrator", error = %e, "net actions channel full; dropping broadcast");
                            }
                        }
                        if let Err(e) = self.action_applier.apply(&action) {
                            warn!(target: "node::orchestrator", error = %e, "local action apply failed");
                        }
                    }
                },
                maybe_action = self.bridge.actions_rx.recv() => {
                    let Some(_action) = maybe_action else { break };
                },
            }
        }
        Ok(())
    }
}
