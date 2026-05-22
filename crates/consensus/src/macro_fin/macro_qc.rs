//! Mode 0 flat MacroQc aggregation (L3 03c-1 / 03d real BLS).

use std::collections::{BTreeSet, HashMap};

use crypto::bls::aggregate::aggregate_sigs;
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    macros::{AggregationMode, MacroCheckpoint, MacroQc},
    primitives::ValidatorId,
    validator::ValidatorSet,
};

use crate::{
    event::{SubnetAggregate, SubnetId},
    macro_fin::{aggregation::mode_a_subnet::try_finalize_mode_a, messages},
};

/// **Deprecated skeleton** — kept so existing re-exports continue to compile.
#[derive(Debug)]
pub struct MacroQcAssembler;

impl MacroQcAssembler {
    /// Skeleton retained for backward-compat.
    #[must_use]
    pub fn placeholder() -> Self {
        Self
    }
}

/// Build a `MacroQc` in `AggregationMode::Mode0Flat` once the signer set
/// reaches ≥ `2f + 1` distinct validators (equal-stake sim assumption).
#[must_use]
pub fn try_finalize_mode0(
    target: Hash32,
    signers: &BTreeSet<ValidatorId>,
    partial_sigs: &HashMap<(Hash32, ValidatorId), BlsSig>,
    set: &ValidatorSet,
    checkpoint: &MacroCheckpoint,
) -> Option<MacroQc> {
    let n = set.entries.len();
    if n == 0 {
        return None;
    }
    let f = (n - 1) / 3;
    let need = 2 * f + 1;
    if signers.len() < need {
        return None;
    }
    let _msg = messages::checkpoint_message(checkpoint);
    let mut bitmap = vec![0u8; n.div_ceil(8)];
    let mut sigs = Vec::with_capacity(signers.len());
    for (i, entry) in set.entries.iter().enumerate() {
        if signers.contains(&entry.id) {
            bitmap[i / 8] |= 1 << (i % 8);
            let sig = *partial_sigs.get(&(target, entry.id))?;
            sigs.push(sig);
        }
    }
    let agg = aggregate_sigs(&sigs).ok()?;
    Some(MacroQc {
        checkpoint_hash: target,
        mode: AggregationMode::Mode0Flat,
        agg: BlsAggSig { sig: agg, bitmap },
    })
}

/// Build a `MacroQc` in `AggregationMode::ModeASubnet` from subnet aggregates.
#[must_use]
pub fn try_finalize_mode_a_from_aggs(
    target: Hash32,
    aggs: &HashMap<SubnetId, SubnetAggregate>,
    set: &ValidatorSet,
    partial_sigs: &HashMap<(Hash32, ValidatorId), BlsSig>,
) -> Option<MacroQc> {
    try_finalize_mode_a(target, aggs, set, partial_sigs)
}

/// Build a `MacroQc` in `AggregationMode::ModeBLeaderless` (same threshold as Mode 0).
#[must_use]
pub fn try_finalize_mode_b(
    target: Hash32,
    signers: &BTreeSet<ValidatorId>,
    partial_sigs: &HashMap<(Hash32, ValidatorId), BlsSig>,
    set: &ValidatorSet,
    checkpoint: &MacroCheckpoint,
) -> Option<MacroQc> {
    let qc = try_finalize_mode0(target, signers, partial_sigs, set, checkpoint)?;
    Some(MacroQc {
        mode: AggregationMode::ModeBLeaderless,
        ..qc
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::bls::{SecretKey, sign::sign};
    use crypto::hash::dst;
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;
    use types::{
        crypto_types::VrfPubkey,
        primitives::{Epoch, Height, StakeWeight},
        validator::{ValidatorEntry, ValidatorIdentity},
    };

    fn vset_with_sks(sks: &[(ValidatorId, crypto::bls::SecretKey)]) -> ValidatorSet {
        let entries = sks
            .iter()
            .map(|(id, sk)| ValidatorEntry {
                id: *id,
                bls_pubkey: sk.public().to_bytes(),
                vrf_pubkey: VrfPubkey::zero(),
                stake: StakeWeight(1),
                identity: ValidatorIdentity {
                    asn: None,
                    cloud: None,
                    region: None,
                },
            })
            .collect();
        ValidatorSet {
            epoch: Epoch(0),
            entries,
            total_stake: StakeWeight(u64::try_from(sks.len()).unwrap_or(0)),
        }
    }

    #[test]
    fn at_threshold_builds_real_aggregate() {
        let mut rng = ChaCha20Rng::from_seed([20; 32]);
        let sks: Vec<_> = (0..3u32)
            .map(|i: u32| {
                let mut id = [0u8; 32];
                id[..4].copy_from_slice(&i.to_be_bytes());
                (ValidatorId(id), SecretKey::random(&mut rng).unwrap())
            })
            .collect();
        let set = vset_with_sks(&sks);
        let cp = MacroCheckpoint {
            height: Height(0),
            epoch: Epoch(0),
            parent: Hash32::zero(),
            micro_root: Hash32([1; 32]),
            hash: Hash32([2; 32]),
        };
        let msg = messages::checkpoint_message(&cp);
        let mut partial_sigs = HashMap::new();
        let mut signers = BTreeSet::new();
        for (id, sk) in &sks {
            signers.insert(*id);
            partial_sigs.insert(
                (cp.hash, *id),
                sign(sk, dst::MACRO_CHECKPOINT, &msg),
            );
        }
        let qc = try_finalize_mode0(cp.hash, &signers, &partial_sigs, &set, &cp).unwrap();
        assert_ne!(qc.agg.sig.0, [0xCD; 96]);
    }
}
