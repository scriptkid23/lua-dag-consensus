//! Mode A subnet aggregation (Ne ≥ 500) — rotated per epoch.

use std::collections::{BTreeSet, HashMap};

use crypto::bls::aggregate::aggregate_sigs;
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    macros::{AggregationMode, MacroQc},
    primitives::ValidatorId,
    validator::ValidatorSet,
};

use crate::{
    event::{SubnetAggregate, SubnetId},
    macro_fin::aggregation::subnet::SubnetAssign,
};

/// Mode A aggregator helpers.
#[derive(Debug, Default)]
pub struct ModeASubnet;

impl ModeASubnet {
    /// Build a subnet aggregate once `signers` reaches the subnet-local `2f+1` threshold.
    #[must_use]
    pub fn try_build_aggregate(
        target: Hash32,
        subnet: SubnetId,
        signers: &BTreeSet<ValidatorId>,
        set: &ValidatorSet,
        assign: &SubnetAssign,
        partial_sigs: &HashMap<(Hash32, ValidatorId), BlsSig>,
    ) -> Option<SubnetAggregate> {
        let subnet_validators: Vec<_> = set
            .entries
            .iter()
            .filter(|e| assign.index_for(&e.id) == subnet.0)
            .collect();
        let n = subnet_validators.len();
        if n == 0 {
            return None;
        }
        let f = (n - 1) / 3;
        let need = 2 * f + 1;
        if signers.len() < need {
            return None;
        }
        let mut bitmap = vec![0u8; set.entries.len().div_ceil(8)];
        let mut sigs = Vec::with_capacity(signers.len());
        for (i, entry) in set.entries.iter().enumerate() {
            if signers.contains(&entry.id) {
                bitmap[i / 8] |= 1 << (i % 8);
                sigs.push(*partial_sigs.get(&(target, entry.id))?);
            }
        }
        let agg = aggregate_sigs(&sigs).ok()?;
        Some(SubnetAggregate {
            subnet,
            checkpoint_hash: target,
            agg: BlsAggSig { sig: agg, bitmap },
        })
    }

    /// Lowest `ValidatorId` in `subnet` acts as the subnet aggregator in sim.
    #[must_use]
    pub fn aggregator_for(subnet: SubnetId, set: &ValidatorSet, assign: &SubnetAssign) -> Option<ValidatorId> {
        set.entries
            .iter()
            .filter(|e| assign.index_for(&e.id) == subnet.0)
            .min_by_key(|e| e.id.as_bytes())
            .map(|e| e.id)
    }
}

/// Union subnet aggregates into a global `MacroQc` when combined stake ≥ `2f+1`.
#[must_use]
pub fn try_finalize_mode_a(
    target: Hash32,
    aggs: &HashMap<SubnetId, SubnetAggregate>,
    set: &ValidatorSet,
    partial_sigs: &HashMap<(Hash32, ValidatorId), BlsSig>,
) -> Option<MacroQc> {
    let n = set.entries.len();
    if n == 0 {
        return None;
    }
    let f = (n - 1) / 3;
    let need = 2 * f + 1;
    let mut bitmap = vec![0u8; n.div_ceil(8)];
    for agg in aggs.values() {
        for (i, _entry) in set.entries.iter().enumerate() {
            let byte = agg.agg.bitmap.get(i / 8).copied().unwrap_or(0);
            if byte & (1 << (i % 8)) != 0 {
                bitmap[i / 8] |= 1 << (i % 8);
            }
        }
    }
    let signer_count = bitmap.iter().map(|b| b.count_ones()).sum::<u32>() as usize;
    if signer_count < need {
        return None;
    }
    let mut sigs = Vec::with_capacity(signer_count);
    for (i, entry) in set.entries.iter().enumerate() {
        let byte = bitmap.get(i / 8).copied().unwrap_or(0);
        if byte & (1 << (i % 8)) != 0 {
            sigs.push(*partial_sigs.get(&(target, entry.id))?);
        }
    }
    let agg = aggregate_sigs(&sigs).ok()?;
    Some(MacroQc {
        checkpoint_hash: target,
        mode: AggregationMode::ModeASubnet,
        agg: BlsAggSig { sig: agg, bitmap },
    })
}
