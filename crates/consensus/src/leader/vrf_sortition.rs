//! Stake-weighted sortition score `y_i · W / (w_i · rep_i)` (spec §8.1).

use crypto::vrf::vrf_to_uniform;
use types::crypto_types::Hash32;

/// Compute the sortition score for a validator.
///
/// Lower score → earlier in the sortition order. Caller picks the
/// minimum score across the active set.
///
/// * `vrf_beta` — the validator's VRF output for this slot's `alpha`.
/// * `total_stake` — Σ stake across the active set.
/// * `own_stake` — this validator's stake.
/// * `reputation` — Shoal reputation (typically in `[0.8, 1.2]`).
#[must_use]
#[allow(clippy::cast_precision_loss)] // stake weights may exceed f64 mantissa; sortition uses coarse ratio
pub fn vrf_sortition_score(
    vrf_beta: &Hash32,
    total_stake: u64,
    own_stake: u64,
    reputation: f64,
) -> f64 {
    let y = vrf_to_uniform(vrf_beta);
    let denom = (own_stake as f64) * reputation;
    if denom == 0.0 {
        f64::INFINITY
    } else {
        y * (total_stake as f64) / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn higher_stake_or_reputation_lowers_score() {
        let beta = Hash32([0xFF; 32]);
        let s_low_rep = vrf_sortition_score(&beta, 1_000, 100, 0.8);
        let s_high_rep = vrf_sortition_score(&beta, 1_000, 100, 1.2);
        assert!(s_high_rep < s_low_rep);
        let s_low_stake = vrf_sortition_score(&beta, 1_000, 100, 1.0);
        let s_high_stake = vrf_sortition_score(&beta, 1_000, 500, 1.0);
        assert!(s_high_stake < s_low_stake);
    }
}
