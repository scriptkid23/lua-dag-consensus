//! Pure-function verifier for `SlashEvidence`.

use types::slashing::SlashEvidence;

use crate::error::Result;

/// Verify a slashing evidence. Skeleton always returns `Ok(())`; plan 03d
/// implements the per-variant verifier.
pub fn verify_evidence(_ev: &SlashEvidence) -> Result<()> {
    Ok(())
}
