//! `lock_macro` §13.5: per-validator locks are consistent. Skeleton:
//! trivially true since `lock_macro` is not yet driven from the SM.

use anyhow::Result;

use crate::world::World;

/// Run the `lock_macro` check.
#[allow(clippy::unnecessary_wraps)]
pub fn check(_world: &World) -> Result<()> {
    Ok(())
}
