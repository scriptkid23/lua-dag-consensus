//! `Persistence` port. Storage adapter (plan 05) implements this trait.

use types::{
    crypto_types::Hash32,
    macros::{MacroCheckpoint, MacroQc},
    micro::MicroQc,
    primitives::Height,
    slashing::SlashEvidence,
};

use crate::error::Result;

/// Persistent storage for finalized artifacts and append-only logs.
pub trait Persistence: Send + Sync {
    /// Persist a MicroQc.
    fn store_micro_qc(&self, qc: &MicroQc) -> Result<()>;

    /// Persist a MacroCheckpoint.
    fn store_macro_checkpoint(&self, cp: &MacroCheckpoint) -> Result<()>;

    /// Persist a MacroQc (finalized).
    fn store_macro_qc(&self, qc: &MacroQc) -> Result<()>;

    /// Append slashing evidence to the immutable log.
    fn append_slash_evidence(&self, ev: &SlashEvidence) -> Result<()>;

    /// Return the macro checkpoint at `height` if known.
    fn macro_checkpoint_at(&self, height: Height) -> Result<Option<MacroCheckpoint>>;

    /// Return the macro QC for `checkpoint_hash` if any.
    fn macro_qc_for(&self, checkpoint_hash: &Hash32) -> Result<Option<MacroQc>>;
}
