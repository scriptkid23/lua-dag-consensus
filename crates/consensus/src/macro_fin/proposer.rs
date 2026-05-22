//! Macro proposer scheduling (primary + backup).

use std::collections::HashMap;

use crypto::hash::{blake3_with_dst, dst};
use types::{
    crypto_types::{Hash32, VrfProof},
    primitives::{Height, ValidatorId},
    validator::ValidatorSet,
};

use crate::leader::{reputation::Reputation, vrf_sortition::vrf_sortition_score};

/// Primary + backup proposer for a given macro window.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposerSchedule {
    /// Macro window height.
    pub height: Height,
    /// Primary proposer.
    pub primary: ValidatorId,
    /// Backup proposer (used after `T_macropropose` timeout in 03c-2).
    pub backup: ValidatorId,
}

/// Public beta for macro proposer sortition at `height`.
#[must_use]
pub fn macro_sortition_beta(beacon: &Hash32, height: Height, validator: &ValidatorId) -> Hash32 {
    let mut buf = Vec::with_capacity(40);
    buf.extend_from_slice(&beacon.0);
    buf.extend_from_slice(&height.0.to_be_bytes());
    buf.extend_from_slice(validator.as_bytes());
    blake3_with_dst(dst::MACRO_PROPOSAL, &buf)
}

impl ProposerSchedule {
    /// Round-robin by height (03c-1 / tests).
    #[must_use]
    pub fn round_robin(set: &ValidatorSet, height: Height) -> Self {
        debug_assert!(!set.entries.is_empty(), "validator set must be non-empty");
        let n = set.entries.len();
        let primary_idx = usize::try_from(height.0).unwrap_or(usize::MAX) % n;
        let backup_idx = usize::try_from(height.0)
            .unwrap_or(usize::MAX)
            .wrapping_add(1)
            % n;
        Self {
            height,
            primary: set.entries[primary_idx].id,
            backup: set.entries[backup_idx].id,
        }
    }

    /// VRF sortition for macro height `height` (03c-2).
    #[must_use]
    pub fn vrf_sortition(
        beacon: &Hash32,
        set: &ValidatorSet,
        height: Height,
        reputation: &HashMap<ValidatorId, Reputation>,
    ) -> Self {
        debug_assert!(!set.entries.is_empty());
        let total = set.total_stake.0;
        let mut scored: Vec<(ValidatorId, f64)> = set
            .entries
            .iter()
            .map(|e| {
                let beta = macro_sortition_beta(beacon, height, &e.id);
                let rep = reputation.get(&e.id).copied().unwrap_or_default().0;
                let score = vrf_sortition_score(&beta, total, e.stake.0, rep);
                (e.id, score)
            })
            .collect();
        scored.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.as_bytes().cmp(b.0.as_bytes()))
        });
        let primary = scored[0].0;
        let backup = scored[1].0;
        Self {
            height,
            primary,
            backup,
        }
    }

    /// Build `VrfProof` bytes from sortition beta (80-byte wire field).
    #[must_use]
    pub fn vrf_proof_for(beacon: &Hash32, height: Height, proposer: &ValidatorId) -> VrfProof {
        let beta = macro_sortition_beta(beacon, height, proposer);
        let mut proof = [0u8; 80];
        proof[..32].copy_from_slice(&beta.0);
        VrfProof(proof)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::{
        crypto_types::BlsPubkey,
        primitives::{Epoch, StakeWeight},
        validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
    };

    fn vset(n: u32) -> ValidatorSet {
        let entries = (0..n)
            .map(|i| {
                let mut id = [0u8; 32];
                id[..4].copy_from_slice(&i.to_be_bytes());
                ValidatorEntry {
                    id: ValidatorId(id),
                    bls_pubkey: BlsPubkey([0; 48]),
                    stake: StakeWeight(1),
                    identity: ValidatorIdentity {
                        asn: None,
                        cloud: None,
                        region: None,
                    },
                }
            })
            .collect();
        ValidatorSet {
            epoch: Epoch(0),
            entries,
            total_stake: StakeWeight(u64::from(n)),
        }
    }

    #[test]
    fn round_robin_primary_rotates_with_height() {
        let set = vset(4);
        let s0 = ProposerSchedule::round_robin(&set, Height(0));
        let s1 = ProposerSchedule::round_robin(&set, Height(1));
        let s4 = ProposerSchedule::round_robin(&set, Height(4));
        assert_eq!(s0.primary, set.entries[0].id);
        assert_eq!(s1.primary, set.entries[1].id);
        assert_eq!(s4.primary, set.entries[0].id, "wraps mod n");
    }

    #[test]
    fn round_robin_backup_is_next_primary() {
        let set = vset(4);
        let s2 = ProposerSchedule::round_robin(&set, Height(2));
        assert_eq!(s2.primary, set.entries[2].id);
        assert_eq!(s2.backup, set.entries[3].id);
        let s3 = ProposerSchedule::round_robin(&set, Height(3));
        assert_eq!(s3.backup, set.entries[0].id, "backup wraps mod n");
    }

    #[test]
    fn vrf_sortition_picks_lowest_score_as_primary() {
        let set = vset(4);
        let beacon = Hash32([0xBE; 32]);
        let reps = HashMap::new();
        let s = ProposerSchedule::vrf_sortition(&beacon, &set, Height(3), &reps);
        let mut scored: Vec<_> = set
            .entries
            .iter()
            .map(|e| {
                let beta = macro_sortition_beta(&beacon, Height(3), &e.id);
                let score = vrf_sortition_score(&beta, set.total_stake.0, e.stake.0, 1.0);
                (e.id, score)
            })
            .collect();
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        assert_eq!(s.primary, scored[0].0);
        assert_eq!(s.backup, scored[1].0);
    }
}
