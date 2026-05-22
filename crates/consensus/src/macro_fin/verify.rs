//! L3 BLS / ECVRF verify helpers.

use std::collections::{BTreeSet, HashMap};

use crypto::{
    bls::{PublicKey, aggregate::verify_aggregate, sign::verify},
    hash::dst,
    vrf::vrf_verify,
};
use types::{
    crypto_types::BlsSig,
    macros::{MacroCheckpoint, MacroProposal, MacroQc},
    primitives::ValidatorId,
    validator::{ValidatorEntry, ValidatorSet},
};

use crate::event::{BlsPartial, SubnetAggregate};

use super::messages;

fn pk_from_entry(entry: &ValidatorEntry) -> Option<PublicKey> {
    PublicKey::from_bytes(&entry.bls_pubkey).ok()
}

fn entry_for<'a>(set: &'a ValidatorSet, id: &ValidatorId) -> Option<&'a ValidatorEntry> {
    set.entries.iter().find(|e| &e.id == id)
}

/// Verify a validator partial BLS signature over the macro checkpoint body.
#[must_use]
pub fn verify_partial(
    set: &ValidatorSet,
    bp: &BlsPartial,
    checkpoint: &MacroCheckpoint,
) -> bool {
    if bp.checkpoint_hash != checkpoint.hash {
        return false;
    }
    let Some(entry) = entry_for(set, &bp.validator) else {
        return false;
    };
    let Some(pk) = pk_from_entry(entry) else {
        return false;
    };
    let msg = messages::checkpoint_message(checkpoint);
    verify(&pk, dst::MACRO_CHECKPOINT, &msg, &bp.sig).is_ok()
}

/// Verify a macro proposer signature and optional ECVRF proof.
#[must_use]
pub fn verify_proposal(set: &ValidatorSet, p: &MacroProposal, vrf_alpha: &[u8]) -> bool {
    let Some(entry) = entry_for(set, &p.proposer) else {
        return false;
    };
    let Some(pk) = pk_from_entry(entry) else {
        return false;
    };
    let msg = messages::proposer_message(&p.proposer, &p.checkpoint);
    if verify(&pk, dst::MACRO_PROPOSER_SIG, &msg, &p.proposer_sig).is_err() {
        return false;
    }
    if !entry.vrf_pubkey.is_zero()
        && vrf_verify(&entry.vrf_pubkey.0, vrf_alpha, &p.vrf_proof).is_err()
    {
        return false;
    }
    true
}

/// Verify a subnet aggregate over partial signers in the bitmap.
#[must_use]
pub fn verify_subnet_agg(
    set: &ValidatorSet,
    agg: &SubnetAggregate,
    checkpoint: &MacroCheckpoint,
    partial_sigs: &HashMap<(types::crypto_types::Hash32, ValidatorId), BlsSig>,
) -> bool {
    let msg = messages::checkpoint_message(checkpoint);
    let mut pks = Vec::new();
    for (i, entry) in set.entries.iter().enumerate() {
        let byte = agg.agg.bitmap.get(i / 8).copied().unwrap_or(0);
        if byte & (1 << (i % 8)) == 0 {
            continue;
        }
        let Some(pk) = pk_from_entry(entry) else {
            return false;
        };
        let key = (agg.checkpoint_hash, entry.id);
        let Some(sig) = partial_sigs.get(&key) else {
            return false;
        };
        if verify(
            &pk,
            dst::MACRO_CHECKPOINT,
            &msg,
            sig,
        )
        .is_err()
        {
            return false;
        }
        pks.push(pk);
    }
    if pks.is_empty() {
        return false;
    }
    verify_aggregate(&pks, dst::MACRO_CHECKPOINT, &msg, &agg.agg.sig).is_ok()
}

/// Verify a `MacroQc` aggregate signature against the checkpoint body.
#[must_use]
pub fn verify_macro_qc(set: &ValidatorSet, qc: &MacroQc, checkpoint: &MacroCheckpoint) -> bool {
    if qc.checkpoint_hash != checkpoint.hash {
        return false;
    }
    let msg = messages::checkpoint_message(checkpoint);
    let mut pks = Vec::new();
    for (i, entry) in set.entries.iter().enumerate() {
        let byte = qc.agg.bitmap.get(i / 8).copied().unwrap_or(0);
        if byte & (1 << (i % 8)) != 0 {
            let Some(pk) = pk_from_entry(entry) else {
                return false;
            };
            pks.push(pk);
        }
    }
    if pks.is_empty() {
        return false;
    }
    verify_aggregate(&pks, dst::MACRO_CHECKPOINT, &msg, &qc.agg.sig).is_ok()
}

/// Collect pubkeys for signers in validator-set order.
#[must_use]
pub fn pubkeys_for_signers(set: &ValidatorSet, signers: &BTreeSet<ValidatorId>) -> Vec<PublicKey> {
    set.entries
        .iter()
        .filter(|e| signers.contains(&e.id))
        .filter_map(|e| pk_from_entry(e))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::bls::{SecretKey, sign::sign};
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;
    use types::{
        crypto_types::{Hash32, VrfPubkey},
        macros::AggregationMode,
        primitives::{Epoch, Height, StakeWeight},
        validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
    };

    fn vset_with_pk(sk: &SecretKey, id: ValidatorId) -> ValidatorSet {
        ValidatorSet {
            epoch: Epoch(0),
            entries: vec![ValidatorEntry {
                id,
                bls_pubkey: sk.public().to_bytes(),
                vrf_pubkey: VrfPubkey::zero(),
                stake: StakeWeight(1),
                identity: ValidatorIdentity {
                    asn: None,
                    cloud: None,
                    region: None,
                },
            }],
            total_stake: StakeWeight(1),
        }
    }

    #[test]
    fn valid_partial_passes_flipped_fails() {
        let mut rng = ChaCha20Rng::from_seed([11; 32]);
        let sk = SecretKey::random(&mut rng).unwrap();
        let id = ValidatorId([5; 32]);
        let set = vset_with_pk(&sk, id);
        let cp = MacroCheckpoint {
            height: Height(0),
            epoch: Epoch(0),
            parent: Hash32::zero(),
            micro_root: Hash32([1; 32]),
            hash: Hash32([9; 32]),
        };
        let msg = messages::checkpoint_message(&cp);
        let sig = sign(&sk, dst::MACRO_CHECKPOINT, &msg);
        let bp = BlsPartial {
            subnet: crate::event::SubnetId(0),
            validator: id,
            checkpoint_hash: cp.hash,
            sig,
        };
        assert!(verify_partial(&set, &bp, &cp));
        let mut bad = bp;
        bad.sig.0[0] ^= 0x01;
        assert!(!verify_partial(&set, &bad, &cp));
    }

    #[test]
    fn verify_macro_qc_with_real_aggregate() {
        let mut rng = ChaCha20Rng::from_seed([12; 32]);
        let sks: Vec<_> = (0..3u32)
            .map(|i| {
                let mut id = [0u8; 32];
                id[..4].copy_from_slice(&i.to_be_bytes());
                let sk = SecretKey::random(&mut rng).unwrap();
                (ValidatorId(id), sk)
            })
            .collect();
        let entries: Vec<_> = sks
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
        let set = ValidatorSet {
            epoch: Epoch(0),
            entries,
            total_stake: StakeWeight(3),
        };
        let cp = MacroCheckpoint {
            height: Height(0),
            epoch: Epoch(0),
            parent: Hash32::zero(),
            micro_root: Hash32([1; 32]),
            hash: Hash32([2; 32]),
        };
        let msg = messages::checkpoint_message(&cp);
        let sigs: Vec<_> = sks
            .iter()
            .map(|(_, sk)| sign(sk, dst::MACRO_CHECKPOINT, &msg))
            .collect();
        let agg = crypto::bls::aggregate::aggregate_sigs(&sigs).unwrap();
        let mut bitmap = vec![0u8; 1];
        for i in 0..3 {
            bitmap[0] |= 1 << i;
        }
        let qc = MacroQc {
            checkpoint_hash: cp.hash,
            mode: AggregationMode::Mode0Flat,
            agg: types::crypto_types::BlsAggSig { sig: agg, bitmap },
        };
        assert!(verify_macro_qc(&set, &qc, &cp));
    }
}
