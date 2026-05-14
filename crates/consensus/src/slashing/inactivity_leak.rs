//! Inactivity leak: 0.5 % / window once unfinalized for 4 windows.

use crate::config::Config;

/// Returns `(bps, should_apply)` for the given consecutive unfinalized
/// window count. Skeleton respects the configured trigger and rate.
#[must_use]
pub fn compute(cfg: &Config, unfinalized_windows: u32) -> (u32, bool) {
    let apply = unfinalized_windows >= cfg.macro_fin.inactivity_leak_trigger_windows;
    (cfg.macro_fin.inactivity_leak_bps_per_window, apply)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_leak_under_threshold() {
        let cfg = Config::default_table_17_1();
        let (rate, apply) = compute(&cfg, 3);
        assert_eq!(rate, 50);
        assert!(!apply);
    }

    #[test]
    fn leak_applied_at_or_above_threshold() {
        let cfg = Config::default_table_17_1();
        let (_rate, apply) = compute(&cfg, 4);
        assert!(apply);
    }
}
