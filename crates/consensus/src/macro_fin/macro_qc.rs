//! Macro QC assembly (mode-aware).

use types::{crypto_types::Hash32, macros::MacroQc};

use crate::{config::Config, error::Result};

/// Builder that collects per-validator (or per-subnet) signatures and
/// emits a `MacroQc` once the stake threshold is reached.
#[derive(Debug)]
pub struct MacroQcAssembler<'a> {
    /// Active config (mode thresholds, etc.).
    pub config: &'a Config,
    /// Checkpoint hash being attested.
    pub target: Hash32,
}

impl<'a> MacroQcAssembler<'a> {
    /// Construct.
    #[must_use]
    pub fn new(config: &'a Config, target: Hash32) -> Self {
        Self { config, target }
    }

    /// Skeleton: never finalizes. Plan 03c implements the modes.
    pub fn try_finalize(&self) -> Result<Option<MacroQc>> {
        let _ = self.config;
        let _ = self.target;
        Ok(None)
    }
}
