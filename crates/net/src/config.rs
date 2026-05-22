//! Network configuration loaded from `config/default.toml` (`[net]` table).

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Top-level `net` config.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NetConfig {
    /// Multiaddrs to listen on (e.g. `"/ip4/0.0.0.0/udp/9000/quic-v1"`).
    pub listen: Vec<String>,
    /// Bootstrap peer multiaddrs (with `p2p/<peer-id>` suffix).
    pub bootstrap: Vec<String>,
    /// Gossipsub parameters.
    pub gossip: GossipConfig,
    /// Peer-manager parameters.
    pub peers: PeerConfig,
    /// Mode-A BLS partial subnet count (`0` = derive at node startup from valset).
    #[serde(default)]
    pub macro_subnet_count: u32,
}

/// Gossipsub knobs we need to override.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GossipConfig {
    /// Heartbeat interval in milliseconds.
    pub heartbeat_ms: u64,
    /// Mesh degree.
    pub mesh_n: usize,
    /// Mesh lower bound.
    pub mesh_n_low: usize,
    /// Mesh upper bound.
    pub mesh_n_high: usize,
}

/// Peer manager knobs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerConfig {
    /// Max concurrent peers.
    pub max_peers: usize,
    /// Ban duration (seconds) once score drops below threshold.
    pub ban_duration_secs: u64,
}

impl NetConfig {
    /// Local-devnet defaults.
    #[must_use]
    pub fn devnet_default() -> Self {
        Self {
            listen: vec![
                "/ip4/0.0.0.0/udp/9000/quic-v1".into(),
                "/ip4/0.0.0.0/tcp/9000".into(),
            ],
            bootstrap: vec![],
            gossip: GossipConfig {
                heartbeat_ms: 700,
                mesh_n: 8,
                mesh_n_low: 6,
                mesh_n_high: 12,
            },
            peers: PeerConfig {
                max_peers: 64,
                ban_duration_secs: 600,
            },
            macro_subnet_count: 0,
        }
    }

    /// Convenience accessor.
    #[must_use]
    pub fn heartbeat(&self) -> Duration {
        Duration::from_millis(self.gossip.heartbeat_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devnet_default_listens_on_quic_and_tcp() {
        let c = NetConfig::devnet_default();
        assert!(c.listen.iter().any(|s| s.contains("quic-v1")));
        assert!(c.listen.iter().any(|s| s.contains("/tcp/")));
    }
}
