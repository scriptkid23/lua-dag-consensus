//! After the scenario, at least one validator stored a `MicroQc`
//! AND at least one validator's blob reached `BlobStatus::Finalized`.

use anyhow::{Result, bail};

use crate::world::World;

/// Run the liveness check.
pub fn check(world: &World) -> Result<()> {
    let any_micro = world.persistence.iter().any(|p| p.any_micro_qc());
    if !any_micro {
        bail!("no validator stored a MicroQc after the run");
    }
    let any_final = world.persistence.iter().any(|p| p.finalized_count() >= 1);
    if !any_final {
        bail!("no validator reached BlobStatus::Finalized after the run");
    }
    Ok(())
}
