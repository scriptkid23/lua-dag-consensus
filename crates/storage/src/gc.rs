//! Tiered GC horizons (hot / warm / cold). Skeleton only — actual
//! pruning lives in a follow-up plan that ties horizons to finalized
//! checkpoints.

use consensus::Config;

/// Plan output: which slot to start pruning from for the cold tier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::struct_field_names)]
pub struct GcPlan {
    /// Last round eligible for cold storage at the time of planning.
    pub cold_horizon_round: u64,
    /// Last round eligible for warm storage.
    pub warm_horizon_round: u64,
    /// Last round eligible for hot storage.
    pub hot_horizon_round: u64,
}

/// Compute the next GC plan given the current micro-head round.
#[must_use]
pub fn plan(cfg: &Config, micro_head_round: u64) -> GcPlan {
    let hot = cfg.storage.gc_hot_horizon_rounds;
    let warm = cfg.storage.gc_warm_horizon_rounds;
    GcPlan {
        hot_horizon_round: micro_head_round.saturating_sub(hot),
        warm_horizon_round: micro_head_round.saturating_sub(warm),
        cold_horizon_round: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizons_subtract_from_head() {
        let cfg = Config::default_table_17_1();
        let plan = plan(&cfg, 1_000);
        assert_eq!(plan.hot_horizon_round, 800);
        assert_eq!(plan.warm_horizon_round, 0);
    }
}
