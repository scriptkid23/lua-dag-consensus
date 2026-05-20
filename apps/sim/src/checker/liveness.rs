//! After the scenario, at least one validator stored a `MicroQc`.

use anyhow::{bail, Result};

use crate::world::World;

/// Run the liveness check.
pub fn check(world: &World) -> Result<()> {
    let any = world.persistence.iter().any(|p| p.any_micro_qc());
    if any {
        Ok(())
    } else {
        bail!("no validator stored a MicroQc after the run")
    }
}
