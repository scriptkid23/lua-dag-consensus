//! Mode 0 flat MacroQc aggregation (L3 03c-1).

use std::collections::{BTreeSet, HashMap};

use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    macros::{AggregationMode, MacroQc},
    primitives::ValidatorId,
    validator::ValidatorSet,
};

use crate::{
    event::{SubnetAggregate, SubnetId},
    macro_fin::aggregation::mode_a_subnet::try_finalize_mode_a,
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
///
/// Returns `None` below threshold so callers stay idempotent across
/// repeated `BlsPartialReceived` events.
///
/// Bitmap layout: one bit per `ValidatorEntry` in declared order
/// (`bitmap[i / 8] |= 1 << (i % 8)` when `entries[i].id ∈ signers`).
/// Aggregate signature is the fixture `[0xCD; 96]` (real BLS aggregate
/// arrives in plan 03d).
#[must_use]
pub fn try_finalize_mode0(
    target: Hash32,
    signers: &BTreeSet<ValidatorId>,
    set: &ValidatorSet,
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
    let mut bitmap = vec![0u8; n.div_ceil(8)];
    for (i, entry) in set.entries.iter().enumerate() {
        if signers.contains(&entry.id) {
            bitmap[i / 8] |= 1 << (i % 8);
        }
    }
    Some(MacroQc {
        checkpoint_hash: target,
        mode: AggregationMode::Mode0Flat,
        agg: BlsAggSig {
            sig: BlsSig([0xCD; 96]),
            bitmap,
        },
    })
}

/// Build a `MacroQc` in `AggregationMode::ModeASubnet` from subnet aggregates.
#[must_use]
pub fn try_finalize_mode_a_from_aggs(
    target: Hash32,
    aggs: &HashMap<SubnetId, SubnetAggregate>,
    set: &ValidatorSet,
) -> Option<MacroQc> {
    try_finalize_mode_a(target, aggs, set)
}

/// Build a `MacroQc` in `AggregationMode::ModeBLeaderless` (same threshold as Mode 0).
#[must_use]
pub fn try_finalize_mode_b(
    target: Hash32,
    signers: &BTreeSet<ValidatorId>,
    set: &ValidatorSet,
) -> Option<MacroQc> {
    let qc = try_finalize_mode0(target, signers, set)?;
    Some(MacroQc {
        mode: AggregationMode::ModeBLeaderless,
        ..qc
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::{
        crypto_types::BlsPubkey,
        primitives::{Epoch, StakeWeight},
        validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet as VSet},
    };

    fn vset(n: u32) -> VSet {
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
        VSet {
            epoch: Epoch(0),
            entries,
            total_stake: StakeWeight(u64::from(n)),
        }
    }

    fn signers(set: &VSet, count: usize) -> BTreeSet<ValidatorId> {
        set.entries.iter().take(count).map(|e| e.id).collect()
    }

    #[test]
    fn below_threshold_returns_none() {
        let set = vset(4);
        let s = signers(&set, 2);
        assert!(try_finalize_mode0(Hash32([1; 32]), &s, &set).is_none());
    }

    #[test]
    fn at_exactly_threshold_returns_some_with_correct_bitmap() {
        let set = vset(4);
        let s = signers(&set, 3);
        let qc = try_finalize_mode0(Hash32([1; 32]), &s, &set).expect("threshold met");
        assert_eq!(qc.mode, AggregationMode::Mode0Flat);
        assert_eq!(qc.agg.bitmap, vec![0b0000_0111]);
        assert_eq!(qc.agg.sig.0, [0xCD; 96]);
    }

    #[test]
    fn empty_validator_set_returns_none() {
        let set = vset(0);
        let s = BTreeSet::new();
        assert!(try_finalize_mode0(Hash32([1; 32]), &s, &set).is_none());
    }
}
