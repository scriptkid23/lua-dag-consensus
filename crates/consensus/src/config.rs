//! Protocol parameter set (whitepaper Table 17.1).
//!
//! Loaded once at startup. Override via [`Config::from_toml_str`] or
//! mutate fields directly for tests.

use serde::{Deserialize, Serialize};

/// All tunable protocol parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Schema version. Loaders must reject unknown values.
    pub schema_version: u32,
    /// Timing knobs.
    pub timing: Timing,
    /// Bullshark micro-ordering knobs.
    pub bullshark: BullsharkParams,
    /// Macro-finality knobs.
    pub macro_fin: MacroFinParams,
    /// Adaptive aggregation knobs.
    pub aggregation: AggregationParams,
    /// Leader / reputation knobs.
    pub leader: LeaderParams,
    /// Slashing penalties.
    pub slashing: SlashingParams,
    /// L4 anchor placeholder.
    pub anchor_l4: AnchorL4Params,
    /// Storage GC horizons.
    pub storage: StorageParams,
}

/// Timing knobs (ms).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Timing {
    /// Round length for micro-ordering.
    pub round_duration_ms: u64,
    /// Macro proposer slot.
    pub t_macropropose_ms: u64,
    /// Subnet aggregation window.
    pub t_subnet_ms: u64,
    /// Canonical macro publish window.
    pub t_canonicalize_ms: u64,
}

/// Bullshark parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BullsharkParams {
    /// Micro committee size C.
    pub micro_committee_size: u32,
    /// Shortcut commit round count.
    pub shortcut_round_count: u32,
    /// Slow-path commit round count.
    pub slow_path_round_count: u32,
    /// Wave round count (always 4).
    pub wave_round_count: u32,
}

/// Macro-finality parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MacroFinParams {
    /// W — micro-slots per macro window.
    pub macro_window_w: u32,
    /// Casper FFG 2-chain depth.
    pub two_chain_depth: u32,
    /// Inactivity leak rate (basis points per window).
    pub inactivity_leak_bps_per_window: u32,
    /// Consecutive unfinalized windows that trigger leak.
    pub inactivity_leak_trigger_windows: u32,
}

/// Aggregation thresholds (spec §9.2, Eq. 9.1/9.2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AggregationParams {
    /// Below this, use Mode 0 flat aggregation.
    pub subnet_flat_threshold: u32,
    /// At/above this, use Mode A subnet aggregation.
    pub subnet_full_threshold: u32,
    /// Dev-only: override `K_e` for sim Mode A scenarios (not for production).
    #[serde(default)]
    pub sim_force_ke: Option<u32>,
}

/// Leader election parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LeaderParams {
    /// Shoal reputation floor (typically 0.8).
    pub reputation_floor: f64,
    /// Shoal reputation ceiling (typically 1.2).
    pub reputation_ceiling: f64,
    /// Reputation EWMA decay factor.
    pub reputation_decay: f64,
}

/// Slashing penalty parameters (basis points).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlashingParams {
    /// Macro equivocation penalty.
    pub equivocation_bps: u32,
    /// Surround/double vote penalty.
    pub double_vote_bps: u32,
    /// Data-availability incident penalty per occurrence.
    pub da_incident_bps: u32,
    /// Per-epoch slashing cap.
    pub slashing_cap_bps: u32,
}

/// L4 anchor parameters (placeholder until L4 lands).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnchorL4Params {
    /// Bitcoin confirmations required for `epoch_finalized`.
    pub btc_confirmations_for_final: u32,
}

/// Storage GC horizons (rounds).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StorageParams {
    /// Hot tier horizon.
    pub gc_hot_horizon_rounds: u64,
    /// Warm tier horizon.
    pub gc_warm_horizon_rounds: u64,
    /// Snapshot interval in macro windows.
    pub snapshot_interval_macros: u64,
}

/// Current expected `schema_version`.
pub const SCHEMA_VERSION: u32 = 1;

impl Config {
    /// Defaults mirroring `config/default.toml`.
    #[must_use]
    pub fn default_table_17_1() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            timing: Timing {
                round_duration_ms: 250,
                t_macropropose_ms: 4_000,
                t_subnet_ms: 2_000,
                t_canonicalize_ms: 8_000,
            },
            bullshark: BullsharkParams {
                micro_committee_size: 256,
                shortcut_round_count: 2,
                slow_path_round_count: 4,
                wave_round_count: 4,
            },
            macro_fin: MacroFinParams {
                macro_window_w: 8,
                two_chain_depth: 2,
                inactivity_leak_bps_per_window: 50,
                inactivity_leak_trigger_windows: 4,
            },
            aggregation: AggregationParams {
                subnet_flat_threshold: 500,
                subnet_full_threshold: 1_000,
                sim_force_ke: None,
            },
            leader: LeaderParams {
                reputation_floor: 0.8,
                reputation_ceiling: 1.2,
                reputation_decay: 0.95,
            },
            slashing: SlashingParams {
                equivocation_bps: 10_000,
                double_vote_bps: 5_000,
                da_incident_bps: 500,
                slashing_cap_bps: 5_000,
            },
            anchor_l4: AnchorL4Params {
                btc_confirmations_for_final: 6,
            },
            storage: StorageParams {
                gc_hot_horizon_rounds: 200,
                gc_warm_horizon_rounds: 10_000,
                snapshot_interval_macros: 256,
            },
        }
    }

    /// Dev-only table: forces Mode A with 8 validators in sim (NOT production).
    #[must_use]
    pub fn sim_mode_a_dev() -> Self {
        let mut c = Self::default_table_17_1();
        c.aggregation.subnet_flat_threshold = 8;
        c.aggregation.sim_force_ke = Some(8);
        c
    }

    /// Parse a TOML string into a `Config`.
    pub fn from_toml_str(input: &str) -> crate::Result<Self> {
        let cfg: Self =
            toml::from_str(input).map_err(|e| crate::Error::InvalidConfig(e.to_string()))?;
        if cfg.schema_version != SCHEMA_VERSION {
            return Err(crate::Error::InvalidConfig(format!(
                "unsupported schema_version {} (expected {})",
                cfg.schema_version, SCHEMA_VERSION
            )));
        }
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_table_17_1_constants() {
        let c = Config::default_table_17_1();
        assert_eq!(c.timing.round_duration_ms, 250);
        assert_eq!(c.macro_fin.macro_window_w, 8);
        assert_eq!(c.bullshark.micro_committee_size, 256);
        assert_eq!(c.aggregation.subnet_flat_threshold, 500);
        assert_eq!(c.aggregation.subnet_full_threshold, 1_000);
        assert_eq!(c.anchor_l4.btc_confirmations_for_final, 6);
    }

    #[test]
    fn unknown_schema_version_rejected() {
        let mut cfg = Config::default_table_17_1();
        cfg.schema_version = 99;
        let toml = toml::to_string(&cfg).expect("serialize");
        let err = Config::from_toml_str(&toml).unwrap_err();
        assert!(matches!(err, crate::Error::InvalidConfig(_)));
    }
}
