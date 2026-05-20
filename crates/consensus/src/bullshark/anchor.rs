//! Anchor selection (private VRF sortition).
//!
//! Whitepaper §8.1: each wave has exactly one **anchor** validator, picked
//! by the minimum stake-weighted VRF-sortition score. The proper protocol
//! uses each validator's ECVRF on `alpha = beacon || wave_id`; while sim
//! vertices don't yet carry VRF proofs, we use a publicly-derivable
//! VRF-equivalent: `beta_i = H(beacon || wave_id || validator_id)`. That
//! function is deterministic, depends on the beacon (so it rotates across
//! waves), and lets every node agree on the same anchor without exchanging
//! VRF proofs. Plan 03b-2 calls out the upgrade to real ECVRF as the next
//! step once vertices carry the proof envelope.

use crypto::hash::{blake3_with_dst, dst};
use types::{crypto_types::Hash32, primitives::ValidatorId, validator::ValidatorSet};

use super::wave::WaveId;
use crate::{
    config::LeaderParams, error::Result, leader::vrf_sortition::vrf_sortition_score,
    ports::RandomnessBeacon,
};

/// Outcome of anchor selection for one wave.
///
/// `anchor_hash` is filled in later when the anchor vertex shows up in the
/// DAG; until then it stays `Hash32::zero()`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnchorChoice {
    /// Wave this anchor belongs to.
    pub wave: WaveId,
    /// Author who won the anchor slot.
    pub author: ValidatorId,
    /// Hash of the anchor vertex (zero until the vertex is observed).
    pub anchor_hash: Hash32,
}

/// Deterministically pick the anchor validator for `wave`.
///
/// Score is `vrf_sortition_score(beta_i, total_stake, w_i, 1.0)` (reputation
/// fixed at 1.0 until Shoal lands). Lowest score wins; ties broken by raw
/// `ValidatorId` byte order to keep this total-order.
pub fn select_anchor(
    wave: WaveId,
    set: &ValidatorSet,
    beacon: &dyn RandomnessBeacon,
    _cfg: &LeaderParams,
) -> Result<AnchorChoice> {
    if set.entries.is_empty() {
        return Err(crate::Error::InvalidConfig(
            "anchor selection on empty validator set".into(),
        ));
    }
    let beacon_val = beacon.current()?;
    let total = set.total_stake.0;
    let mut best: Option<(ValidatorId, f64)> = None;
    for entry in &set.entries {
        let beta = sortition_beta(&beacon_val, wave, &entry.id);
        let score = vrf_sortition_score(&beta, total, entry.stake.0, 1.0);
        best = Some(match best {
            None => (entry.id, score),
            Some((prev_id, prev_score)) => {
                match score
                    .partial_cmp(&prev_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                {
                    std::cmp::Ordering::Less => (entry.id, score),
                    std::cmp::Ordering::Equal if entry.id.as_bytes() < prev_id.as_bytes() => {
                        (entry.id, score)
                    }
                    _ => (prev_id, prev_score),
                }
            }
        });
    }
    let (author, _) = best.expect("non-empty set guarantees a winner");
    Ok(AnchorChoice {
        wave,
        author,
        anchor_hash: Hash32::zero(),
    })
}

fn sortition_beta(beacon: &Hash32, wave: WaveId, validator: &ValidatorId) -> Hash32 {
    let mut buf = Vec::with_capacity(32 + 8 + 32);
    buf.extend_from_slice(beacon.as_bytes());
    buf.extend_from_slice(&wave.0.to_be_bytes());
    buf.extend_from_slice(validator.as_bytes());
    blake3_with_dst(dst::BEACON, &buf)
}
