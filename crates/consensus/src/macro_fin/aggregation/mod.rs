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

/// Whitepaper Eq. 9.1: subnet count from active validator count `n_e`.
#[must_use]
pub fn compute_ke(cfg: &Config, n_e: u32) -> Ke {
    if let Some(k) = cfg.aggregation.sim_force_ke {
        return Ke(k);
    }
    if n_e < cfg.aggregation.subnet_flat_threshold {
        return Ke(0);
    }
    Ke(n_e.div_ceil(128).min(32))
}

/// Mode A is active when `K_e >= 4` (whitepaper §9.2).
#[must_use]
pub fn mode_a_active(ke: Ke) -> bool {
    ke.0 >= 4
}

/// Threshold-based mode before runtime Mode B override.
#[must_use]
pub fn select_mode(cfg: &Config, n_e: u32) -> AggregationMode {
    let ke = compute_ke(cfg, n_e);
    if mode_a_active(ke) {
        AggregationMode::ModeASubnet
    } else {
        AggregationMode::Mode0Flat
    }
}

#[cfg(test)]
mod ke_tests {
    use super::*;

    #[test]
    fn ke_is_zero_below_threshold() {
        let cfg = Config::default_table_17_1();
        assert_eq!(compute_ke(&cfg, 499), Ke(0));
        assert_eq!(compute_ke(&cfg, 100), Ke(0));
    }

    #[test]
    fn ke_scales_and_caps_at_32() {
        let cfg = Config::default_table_17_1();
        assert_eq!(compute_ke(&cfg, 500), Ke(4));
        assert_eq!(compute_ke(&cfg, 1000), Ke(8));
        assert_eq!(compute_ke(&cfg, 10_000), Ke(32));
    }

    #[test]
    fn sim_force_ke_overrides_formula() {
        let mut cfg = Config::default_table_17_1();
        cfg.aggregation.sim_force_ke = Some(8);
        assert_eq!(compute_ke(&cfg, 4), Ke(8));
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
