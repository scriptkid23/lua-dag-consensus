//! MicroQc aggregation (≥ ⌈2/3·C⌉).

use types::{crypto_types::Hash32, micro::MicroQc};

use crate::{config::Config, error::Result};

/// Builder that collects partial signatures over a `MicroCheckpoint`
/// hash and emits a [`MicroQc`] once stake threshold is reached.
#[derive(Debug)]
pub struct MicroQcBuilder<'a> {
    /// Reference to active config.
    pub config: &'a Config,
    /// Hash of the checkpoint being attested.
    pub target: Hash32,
}

impl<'a> MicroQcBuilder<'a> {
    /// New builder targeting `target`.
    #[must_use]
    pub fn new(config: &'a Config, target: Hash32) -> Self {
        Self { config, target }
    }

    /// Attempt to finalize the QC. Returns `Ok(None)` until threshold is
    /// reached; `Ok(Some(qc))` once it is.
    ///
    /// Skeleton: always returns `Ok(None)`. Plan 03b implements the rule.
    pub fn try_finalize(&self) -> Result<Option<MicroQc>> {
        let _ = self.config;
        let _ = self.target;
        Ok(None)
    }
}
