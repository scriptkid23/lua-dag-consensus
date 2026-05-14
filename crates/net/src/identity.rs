//! Mapping between libp2p `PeerId` (used at the wire layer) and
//! consensus `ValidatorId` (used inside the SM).
//!
//! The mapping is provided by the host — typically derived from the
//! validator BLS public key registered for the current epoch.

use std::collections::HashMap;

use libp2p::PeerId;
use types::primitives::ValidatorId;

/// Bi-directional, epoch-scoped peer / validator map.
#[derive(Debug, Default)]
pub struct IdentityMap {
    fwd: HashMap<PeerId, ValidatorId>,
    bwd: HashMap<ValidatorId, PeerId>,
}

impl IdentityMap {
    /// New empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a `(peer, validator)` pair. Overwrites any existing entry
    /// on either side.
    pub fn insert(&mut self, peer: PeerId, validator: ValidatorId) {
        if let Some(prev_v) = self.fwd.insert(peer, validator) {
            self.bwd.remove(&prev_v);
        }
        if let Some(prev_p) = self.bwd.insert(validator, peer) {
            self.fwd.remove(&prev_p);
        }
    }

    /// Look up validator id for a peer.
    #[must_use]
    pub fn validator(&self, peer: &PeerId) -> Option<&ValidatorId> {
        self.fwd.get(peer)
    }

    /// Look up peer id for a validator.
    #[must_use]
    pub fn peer(&self, validator: &ValidatorId) -> Option<&PeerId> {
        self.bwd.get(validator)
    }

    /// Drop a peer from both directions.
    pub fn forget(&mut self, peer: &PeerId) {
        if let Some(v) = self.fwd.remove(peer) {
            self.bwd.remove(&v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    #[test]
    fn insert_and_lookup_round_trip() {
        let kp = Keypair::generate_ed25519();
        let peer = PeerId::from_public_key(&kp.public());
        let v = ValidatorId([1; 32]);
        let mut m = IdentityMap::new();
        m.insert(peer, v);
        assert_eq!(m.validator(&peer), Some(&v));
        assert_eq!(m.peer(&v), Some(&peer));
    }

    #[test]
    fn forget_removes_both_directions() {
        let kp = Keypair::generate_ed25519();
        let peer = PeerId::from_public_key(&kp.public());
        let v = ValidatorId([1; 32]);
        let mut m = IdentityMap::new();
        m.insert(peer, v);
        m.forget(&peer);
        assert!(m.validator(&peer).is_none());
        assert!(m.peer(&v).is_none());
    }
}
