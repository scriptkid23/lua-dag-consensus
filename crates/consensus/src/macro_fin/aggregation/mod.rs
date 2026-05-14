//! Adaptive aggregation (Mode 0, A, B). Whitepaper §9.2.

pub mod mode0_flat;
pub mod mode_a_subnet;
pub mod mode_b_leaderless;
pub mod subnet;

pub use mode_a_subnet::ModeASubnet;
pub use mode_b_leaderless::ModeBLeaderless;
pub use mode0_flat::Mode0Flat;
pub use subnet::SubnetAssign;

use crate::config::Config;

/// Aggregation mode chosen for a window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AggregationMode {
    /// Flat (Ne < 500).
    Mode0Flat,
    /// Subnet (Ne ≥ 500).
    ModeASubnet,
    /// Leaderless fallback.
    ModeBLeaderless,
}

/// Number of subnets `Ke` for Mode A (Eq. 9.1).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ke(pub u32);

/// Select the aggregation mode given active-validator count `n_e`.
///
/// Skeleton: thresholds-only — no Mode B fallback logic. Real selection
/// also factors proposer-availability and is implemented in plan 03c.
#[must_use]
pub fn select_mode(cfg: &Config, n_e: u32) -> AggregationMode {
    if n_e < cfg.aggregation.subnet_flat_threshold {
        AggregationMode::Mode0Flat
    } else {
        AggregationMode::ModeASubnet
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_thresholds() {
        let cfg = Config::default_table_17_1();
        assert_eq!(select_mode(&cfg, 100), AggregationMode::Mode0Flat);
        assert_eq!(select_mode(&cfg, 499), AggregationMode::Mode0Flat);
        assert_eq!(select_mode(&cfg, 500), AggregationMode::ModeASubnet);
        assert_eq!(select_mode(&cfg, 5_000), AggregationMode::ModeASubnet);
    }
}
