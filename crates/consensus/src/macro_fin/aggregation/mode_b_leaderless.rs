//! Mode B leaderless fallback (proposer missed both primary and backup slots).

use types::macros::MacroQc;

/// Mode B aggregator state.
#[derive(Debug, Default)]
pub struct ModeBLeaderless;

/// Pick the canonical `MacroQc` among competing candidates.
///
/// Whitepaper: maximize signed stake; tie-break by lexicographic bitmap order
/// (prefer smaller bitmap bytes).
#[must_use]
pub fn pick_canonical(candidates: &[MacroQc]) -> Option<MacroQc> {
    candidates.iter().max_by(|a, b| {
        let sa = signed_stake_from_bitmap(&a.agg.bitmap);
        let sb = signed_stake_from_bitmap(&b.agg.bitmap);
        sa.cmp(&sb).then_with(|| b.agg.bitmap.cmp(&a.agg.bitmap))
    }).cloned()
}

/// Equal-stake sim: one vote per set bit.
fn signed_stake_from_bitmap(bitmap: &[u8]) -> u64 {
    u64::from(bitmap.iter().map(|b| b.count_ones()).sum::<u32>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::{
        crypto_types::{BlsAggSig, BlsSig, Hash32},
        macros::{AggregationMode, MacroQc},
    };

    #[test]
    fn pick_canonical_prefers_higher_stake() {
        let low = MacroQc {
            checkpoint_hash: Hash32([1; 32]),
            mode: AggregationMode::ModeBLeaderless,
            agg: BlsAggSig {
                sig: BlsSig([0; 96]),
                bitmap: vec![0b0000_0011],
            },
        };
        let high = MacroQc {
            checkpoint_hash: Hash32([1; 32]),
            mode: AggregationMode::ModeBLeaderless,
            agg: BlsAggSig {
                sig: BlsSig([0; 96]),
                bitmap: vec![0b0000_1111],
            },
        };
        let picked = pick_canonical(&[low, high.clone()]).expect("pick");
        assert_eq!(picked.agg.bitmap, high.agg.bitmap);
    }

    #[test]
    fn pick_canonical_lex_tiebreak_on_equal_stake() {
        let a = MacroQc {
            checkpoint_hash: Hash32([1; 32]),
            mode: AggregationMode::ModeBLeaderless,
            agg: BlsAggSig {
                sig: BlsSig([0; 96]),
                bitmap: vec![0b0000_1010],
            },
        };
        let b = MacroQc {
            checkpoint_hash: Hash32([1; 32]),
            mode: AggregationMode::ModeBLeaderless,
            agg: BlsAggSig {
                sig: BlsSig([0; 96]),
                bitmap: vec![0b0000_0101],
            },
        };
        let picked = pick_canonical(&[a.clone(), b.clone()]).expect("pick");
        assert_eq!(picked.agg.bitmap, b.agg.bitmap);
    }
}
