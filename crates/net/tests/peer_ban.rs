//! Peer-manager ban behaviour.

use libp2p::{PeerId, identity::Keypair};
use net::{
    config::PeerConfig,
    peers::{PeerManager, Reason},
};

fn peer() -> PeerId {
    PeerId::from_public_key(&Keypair::generate_ed25519().public())
}

#[test]
fn ban_only_after_score_below_threshold() {
    let mut m = PeerManager::new(PeerConfig {
        max_peers: 8,
        ban_duration_secs: 60,
    });
    let p = peer();
    m.on_connected(p);
    // Small infractions should not ban yet.
    assert!(!m.adjust(&p, -10, Reason::SlowDelivery));
    assert!(!m.adjust(&p, -50, Reason::InvalidMessage));
    // Cross the threshold.
    assert!(m.adjust(&p, -1_000, Reason::Equivocation));
}
