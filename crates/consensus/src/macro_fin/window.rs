//! Macro window cadence (W micro-slots per window).

use types::primitives::{Height, Round};

use crate::config::Config;

/// One macro window covering `W` consecutive micro-slots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MacroWindow {
    /// Window height.
    pub height: Height,
}

impl MacroWindow {
    /// Window containing `round`, given the active config.
    #[must_use]
    pub fn of_round(cfg: &Config, round: Round) -> Self {
        let w = u64::from(cfg.macro_fin.macro_window_w);
        Self {
            height: Height(round.0 / w),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_to_window_uses_w_from_config() {
        let cfg = Config::default_table_17_1();
        assert_eq!(MacroWindow::of_round(&cfg, Round(15)).height, Height(1));
        assert_eq!(MacroWindow::of_round(&cfg, Round(16)).height, Height(2));
    }
}
