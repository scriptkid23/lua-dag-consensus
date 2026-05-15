//! "No two conflicting `MacroQc` at the same height are persisted by any
//! validator." Skeleton: trivially true since no QC is produced.

use anyhow::Result;

use crate::world::World;

/// Run the safety check.
#[allow(clippy::unnecessary_wraps)]
pub fn check(_world: &World) -> Result<()> {
    Ok(())
}
