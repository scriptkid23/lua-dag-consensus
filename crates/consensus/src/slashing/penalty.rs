//! Penalty math (basis-point arithmetic).

use crate::config::Config;

/// Penalty kind for accounting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Penalty {
    /// Macro equivocation (100 %).
    Equivocation,
    /// Surround / double-vote (50 %).
    DoubleVote,
    /// Data-availability incident (5 % per incident).
    DaIncident,
    /// Inactivity leak (configurable per-window bps).
    InactivityLeak,
}

impl Penalty {
    /// Penalty in basis points (`10_000 == 100 %`).
    #[must_use]
    pub fn bps(self, cfg: &Config) -> u32 {
        match self {
            Self::Equivocation => cfg.slashing.equivocation_bps,
            Self::DoubleVote => cfg.slashing.double_vote_bps,
            Self::DaIncident => cfg.slashing.da_incident_bps,
            Self::InactivityLeak => cfg.macro_fin.inactivity_leak_bps_per_window,
        }
    }

    /// Clamp the cumulative penalty to the configured per-epoch cap.
    #[must_use]
    pub fn cap(cfg: &Config, cumulative_bps: u32) -> u32 {
        cumulative_bps.min(cfg.slashing.slashing_cap_bps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivocation_is_full_slash() {
        let cfg = Config::default_table_17_1();
        assert_eq!(Penalty::Equivocation.bps(&cfg), 10_000);
    }

    #[test]
    fn cap_respected() {
        let cfg = Config::default_table_17_1();
        assert_eq!(Penalty::cap(&cfg, 20_000), 5_000);
    }
}
