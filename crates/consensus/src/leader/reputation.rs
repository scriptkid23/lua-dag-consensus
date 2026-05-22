//! Shoal reputation `[0.8, 1.2]` — EWMA-style update.

use crate::config::Config;

/// Clamped reputation value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reputation(pub f64);

impl Default for Reputation {
    fn default() -> Self {
        Self::neutral()
    }
}

impl Reputation {
    /// Neutral starting reputation.
    #[must_use]
    pub fn neutral() -> Self {
        Self(1.0)
    }

    /// Apply an observation in `[0.0, 1.0]` (1 = perfect uptime,
    /// 0 = miss). Updates via EWMA, then clamps to `[floor, ceiling]`.
    #[must_use]
    pub fn updated(self, cfg: &Config, observation: f64) -> Self {
        let decay = cfg.leader.reputation_decay;
        let target = cfg.leader.reputation_floor
            + observation.clamp(0.0, 1.0)
                * (cfg.leader.reputation_ceiling - cfg.leader.reputation_floor);
        let next = decay * self.0 + (1.0 - decay) * target;
        Self(next.clamp(cfg.leader.reputation_floor, cfg.leader.reputation_ceiling))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reputation_clamps_inside_band() {
        let cfg = Config::default_table_17_1();
        let r = Reputation::neutral();
        let r1 = r.updated(&cfg, 1.0);
        assert!(r1.0 >= cfg.leader.reputation_floor);
        assert!(r1.0 <= cfg.leader.reputation_ceiling);
        let r2 = r.updated(&cfg, 0.0);
        assert!(r2.0 >= cfg.leader.reputation_floor);
    }
}
