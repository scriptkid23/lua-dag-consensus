//! Macro proposer scheduling (primary + backup).

use types::{
    primitives::{Height, ValidatorId},
    validator::ValidatorSet,
};

/// Primary + backup proposer for a given macro window.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposerSchedule {
    /// Macro window height.
    pub height: Height,
    /// Primary proposer.
    pub primary: ValidatorId,
    /// Backup proposer (used after `T_macropropose` timeout in 03c-2; recorded but unused in 03c-1).
    pub backup: ValidatorId,
}

impl ProposerSchedule {
    /// Round-robin by height: `primary = entries[h mod n]`, `backup = entries[(h+1) mod n]`.
    ///
    /// 03c-1 selection (whitepaper L3 §9 simplification — real ECVRF sortition lands in 03c-2).
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
}
