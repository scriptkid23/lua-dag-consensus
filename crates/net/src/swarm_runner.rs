//! Live libp2p swarm wired to consensus `Event`/`Action` channels (spec §4.1).
//!
//! **Observability:** lifecycle logs use tracing target **`net::swarm`** (`INFO`
//! for listens / connects / gossipsub topic subscriptions; `WARN` for dial and
//! handshake failures). Example: `RUST_LOG=info,net::swarm=info`.
//!
//! Single-task event loop driving:
//!   * inbound gossipsub messages → `gossip_wire::inbound_message` → `events_tx`
//!   * `actions_rx` → `gossip_wire::outbound_broadcast` → `swarm.publish`
//!   * listen-bind progress → `ready` watch flips `true` once every configured
//!     listen addr has produced a `NewListenAddr` event; `listen_addrs` is
//!     updated with every observed address so callers (admin endpoints, tests)
//!     can learn the bound port when `:0` was requested.

use std::collections::HashSet;
use std::time::Duration;

use anyhow::{Context, Result};
use consensus::action::Action;
use consensus::event::{Event, SubnetId};
use futures::StreamExt;
use libp2p::{
    Multiaddr, PeerId, Swarm,
    gossipsub::{self, MessageAuthenticity},
    identity::Keypair,
    swarm::{NetworkBehaviour, SwarmEvent},
};
use tokio::sync::{mpsc, watch};

use crate::NetConfig;
use crate::gossip::Topic;
use crate::gossip_wire;
use crate::transport::build_transport_tcp_only;

const EVENT_BUFFER: usize = 1024;

/// Internal behaviour wrapping just gossipsub for the devnet profile.
#[derive(NetworkBehaviour)]
struct DevnetBehaviour {
    gossipsub: gossipsub::Behaviour,
}

/// Handle returned by [`spawn_gossip_tasks`].
#[derive(Debug)]
pub struct GossipSpawn {
    /// `PeerId` of the local swarm. Stable across the task's lifetime.
    pub local_peer_id: PeerId,
    /// Inbound events decoded from gossip; the orchestrator reads from this.
    pub events_rx: mpsc::Receiver<Event>,
    /// Readiness signal — flips `true` once every listen addr has bound.
    pub ready: watch::Receiver<bool>,
    /// Snapshot of every `NewListenAddr` the swarm has reported. Useful for
    /// admin endpoints and tests that requested `:0`.
    pub listen_addrs: watch::Receiver<Vec<Multiaddr>>,
    /// Handle to the running swarm task. Drop or await on shutdown.
    pub handle: tokio::task::JoinHandle<()>,
}

/// Spawn the gossipsub swarm task.
///
/// Returns once listen addrs are queued — readiness flips later on the
/// `ready` watch. Errors here are fatal to startup: we cannot claim live
/// mode without a transport, a behaviour, and at least one listen socket.
///
/// `async` is retained for API symmetry with callers that already `.await`
/// the spawn; the body itself performs no `.await`.
#[allow(clippy::too_many_lines, clippy::unused_async)]
pub async fn spawn_gossip_tasks(
    keypair: Keypair,
    net_cfg: NetConfig,
    mut actions_rx: mpsc::Receiver<Action>,
) -> Result<GossipSpawn> {
    let transport =
        build_transport_tcp_only(&keypair).context("build TCP+Noise+Yamux transport")?;

    let gossip_cfg = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_millis(net_cfg.gossip.heartbeat_ms))
        .validation_mode(gossipsub::ValidationMode::Strict)
        .mesh_n(net_cfg.gossip.mesh_n)
        .mesh_n_low(net_cfg.gossip.mesh_n_low)
        .mesh_n_high(net_cfg.gossip.mesh_n_high)
        .build()
        .map_err(|e| anyhow::anyhow!("gossipsub config: {e}"))?;

    let mut gossipsub =
        gossipsub::Behaviour::new(MessageAuthenticity::Signed(keypair.clone()), gossip_cfg)
            .map_err(|e| anyhow::anyhow!("gossipsub behaviour: {e}"))?;

    for topic in subscribe_set() {
        gossipsub
            .subscribe(&topic.ident())
            .with_context(|| format!("subscribe {topic:?}"))?;
    }

    let local_peer_id = keypair.public().to_peer_id();
    let mut swarm = Swarm::new(
        transport,
        DevnetBehaviour { gossipsub },
        local_peer_id,
        libp2p::swarm::Config::with_tokio_executor(),
    );

    // Parse + register listen addrs. Errors here are fatal: we cannot claim
    // live mode without listening.
    let mut pending_listen: HashSet<Multiaddr> = HashSet::new();
    for addr in &net_cfg.listen {
        let ma: Multiaddr = addr
            .parse()
            .with_context(|| format!("invalid listen multiaddr `{addr}`"))?;
        swarm
            .listen_on(ma.clone())
            .with_context(|| format!("listen_on {ma}"))?;
        pending_listen.insert(ma);
    }
    let pending_count = pending_listen.len();

    // Dial bootstrap peers (best-effort; failures are logged, not fatal).
    for addr in &net_cfg.bootstrap {
        match addr.parse::<Multiaddr>() {
            Ok(ma) => {
                if let Err(e) = swarm.dial(ma.clone()) {
                    tracing::warn!(target: "net::swarm", %ma, error = %e, "bootstrap dial failed");
                }
            }
            Err(e) => {
                tracing::warn!(target: "net::swarm", addr = %addr, error = %e, "invalid bootstrap multiaddr");
            }
        }
    }

    let (events_tx, events_rx) = mpsc::channel::<Event>(EVENT_BUFFER);
    let (ready_tx, ready_rx) = watch::channel(false);
    let (listen_tx, listen_rx) = watch::channel(Vec::<Multiaddr>::new());

    let handle = tokio::spawn(async move {
        // `port :0` resolves to many concrete addresses. The pending-listen
        // logic uses exact-match strip-/p2p comparison to drain the set; for
        // wildcard `:0` we instead flip ready after the first NewListenAddr
        // event per requested listen entry. We approximate this by tracking
        // how many addresses we've observed and flipping ready when that
        // count meets the requested-listen count.
        let mut observed = 0usize;
        loop {
            tokio::select! {
                ev = swarm.select_next_some() => match ev {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        observed += 1;
                        tracing::info!(
                            target: "net::swarm",
                            address = %address,
                            observed_listen_addrs = observed,
                            "listening on address",
                        );
                        // Update the public snapshot of bound listen addrs.
                        listen_tx.send_modify(|v| {
                            if !v.iter().any(|a| a == &address) {
                                v.push(address.clone());
                            }
                        });
                        let stripped = strip_p2p(&address);
                        pending_listen.retain(|a| {
                            // Drop exact matches (no /p2p suffix on stored entry).
                            // For wildcard `/tcp/0` entries the exact-match
                            // approach never drains, so fall back to the
                            // `observed >= pending_count` check below.
                            strip_p2p(a) != stripped
                        });
                        if pending_listen.is_empty() || observed >= pending_count {
                            let _ = ready_tx.send(true);
                        }
                    }
                    SwarmEvent::ConnectionEstablished {
                        peer_id,
                        endpoint,
                        num_established,
                        ..
                    } => {
                        let distinct_peers = swarm.connected_peers().count();
                        tracing::info!(
                            target: "net::swarm",
                            remote_peer_id = %peer_id,
                            endpoint = ?endpoint,
                            connection_count_with_peer = num_established.get(),
                            distinct_connected_peer_count = distinct_peers,
                            "p2p connection established — peers can exchange protocols (noise/yamux)",
                        );
                    }
                    SwarmEvent::ConnectionClosed {
                        peer_id,
                        endpoint,
                        num_established,
                        cause,
                        ..
                    } => {
                        let distinct_peers = swarm.connected_peers().count();
                        tracing::info!(
                            target: "net::swarm",
                            remote_peer_id = %peer_id,
                            endpoint = ?endpoint,
                            remaining_connections_with_peer = num_established,
                            distinct_connected_peer_count = distinct_peers,
                            cause = ?cause,
                            "p2p connection closed",
                        );
                    }
                    SwarmEvent::OutgoingConnectionError {
                        peer_id,
                        error,
                        ..
                    } => {
                        tracing::warn!(
                            target: "net::swarm",
                            peer_id = ?peer_id,
                            error = %error,
                            "outgoing dial failed — check bootstrap multiaddrs / dns / firewall",
                        );
                    }
                    SwarmEvent::IncomingConnectionError {
                        send_back_addr,
                        error,
                        ..
                    } => {
                        tracing::warn!(
                            target: "net::swarm",
                            send_back_addr = %send_back_addr,
                            error = %error,
                            "incoming connection handshake failed",
                        );
                    }
                    SwarmEvent::Behaviour(DevnetBehaviourEvent::Gossipsub(gs_ev)) => match gs_ev {
                        gossipsub::Event::Message { message, .. } => {
                            match gossip_wire::inbound_message(message.topic.as_str(), &message.data)
                            {
                                Ok(Some(event)) => {
                                    if events_tx.send(event).await.is_err() {
                                        tracing::warn!("events_rx dropped; shutting swarm task");
                                        break;
                                    }
                                }
                                Ok(None) => {} // topic recognized but no Event mapping yet
                                Err(e) => tracing::warn!(error = %e, "inbound decode failed"),
                            }
                        }
                        gossipsub::Event::Subscribed { peer_id, topic } => {
                            tracing::info!(
                                target: "net::swarm",
                                remote_peer_id = %peer_id,
                                topic = ?topic,
                                "gossipsub peer subscribed to topic — overlay mesh forming",
                            );
                        }
                        gossipsub::Event::Unsubscribed { peer_id, topic } => {
                            tracing::info!(
                                target: "net::swarm",
                                remote_peer_id = %peer_id,
                                topic = ?topic,
                                "gossipsub peer unsubscribed from topic",
                            );
                        }
                        gossipsub::Event::GossipsubNotSupported { peer_id } => {
                            tracing::warn!(
                                target: "net::swarm",
                                remote_peer_id = %peer_id,
                                "peer connected without gossipsub — wrong protocol stack?",
                            );
                        }
                        gossipsub::Event::SlowPeer {
                            peer_id,
                            failed_messages,
                        } => {
                            tracing::warn!(
                                target: "net::swarm",
                                remote_peer_id = %peer_id,
                                ?failed_messages,
                                "gossipsub peer marked slow",
                            );
                        }
                    },
                    _ => {}
                },
                maybe_action = actions_rx.recv() => match maybe_action {
                    None => break, // upstream closed
                    Some(action) => match gossip_wire::outbound_broadcast(&action) {
                        Ok(Some((topic, payload))) => {
                            if let Err(e) = swarm
                                .behaviour_mut()
                                .gossipsub
                                .publish(topic.ident(), payload)
                            {
                                tracing::warn!(error = %e, ?topic, "gossipsub publish failed");
                            }
                        }
                        Ok(None) => {
                            tracing::debug!(?action, "non-broadcast action reached swarm; ignored");
                        }
                        Err(e) => tracing::warn!(error = %e, ?action, "outbound encode failed"),
                    },
                },
            }
        }
    });

    Ok(GossipSpawn {
        local_peer_id,
        events_rx,
        ready: ready_rx,
        listen_addrs: listen_rx,
        handle,
    })
}

fn subscribe_set() -> [Topic; 7] {
    [
        Topic::CertifiedVertex,
        Topic::MicroQc,
        Topic::MacroProposal,
        Topic::SubnetAggregate,
        Topic::MacroQc,
        Topic::SlashEvidence,
        Topic::BlsPartial(SubnetId(0)),
    ]
}

fn strip_p2p(addr: &Multiaddr) -> Multiaddr {
    let mut out = Multiaddr::empty();
    for proto in addr {
        if matches!(proto, libp2p::multiaddr::Protocol::P2p(_)) {
            continue;
        }
        out.push(proto);
    }
    out
}
