//! Peer manager: tracks connected peers, applies score updates, bans.

use std::collections::HashMap;

use libp2p::PeerId;

pub mod discovery;
pub mod scoring;

pub use discovery::DiscoveryConfig;
pub use scoring::{PeerScore, Reason};

use crate::config::PeerConfig;

/// In-memory peer registry.
#[derive(Debug)]
pub struct PeerManager {
    cfg: PeerConfig,
    peers: HashMap<PeerId, PeerScore>,
}

impl PeerManager {
    /// New manager with the supplied config.
    #[must_use]
    pub fn new(cfg: PeerConfig) -> Self {
        Self {
            cfg,
            peers: HashMap::with_capacity(cfg.max_peers),
        }
    }

    /// Note a peer as connected (idempotent).
    pub fn on_connected(&mut self, peer: PeerId) {
        self.peers.entry(peer).or_insert_with(PeerScore::neutral);
    }

    /// Drop a peer (disconnected or banned).
    pub fn on_disconnected(&mut self, peer: &PeerId) {
        self.peers.remove(peer);
    }

    /// Apply a score adjustment; returns `true` if the peer should be
    /// banned (score crossed below threshold).
    pub fn adjust(&mut self, peer: &PeerId, delta: i32, reason: Reason) -> bool {
        if let Some(s) = self.peers.get_mut(peer) {
            s.adjust(delta, reason);
            s.is_banned()
        } else {
            false
        }
    }

    /// Number of currently tracked peers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    /// True iff no peers are tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    /// Returns true when the manager is at its `max_peers` cap.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.peers.len() >= self.cfg.max_peers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    fn peer() -> PeerId {
        PeerId::from_public_key(&Keypair::generate_ed25519().public())
    }

    #[test]
    fn connect_then_disconnect_drops_peer() {
        let mut m = PeerManager::new(PeerConfig {
            max_peers: 4,
            ban_duration_secs: 60,
        });
        let p = peer();
        m.on_connected(p);
        assert_eq!(m.len(), 1);
        m.on_disconnected(&p);
        assert_eq!(m.len(), 0);
    }

    #[test]
    fn enough_negative_score_triggers_ban() {
        let mut m = PeerManager::new(PeerConfig {
            max_peers: 4,
            ban_duration_secs: 60,
        });
        let p = peer();
        m.on_connected(p);
        // Single large negative delta should cross the threshold.
        let banned = m.adjust(&p, -1_000, Reason::Equivocation);
        assert!(banned);
    }
}
