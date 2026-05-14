//! Macro equivocation detector (100 % slash).

use types::slashing::MacroEquivocation;

use crate::error::Result;

/// Verify a macro-equivocation evidence. Skeleton no-op.
pub fn verify(_ev: &MacroEquivocation) -> Result<()> {
    Ok(())
}
