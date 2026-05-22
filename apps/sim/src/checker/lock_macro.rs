//! `lock_macro` §13.5: for every macro height, all validators that adopted a
//! `MacroCheckpoint` at that height adopted the same `(checkpoint_hash, MacroQc)`.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use consensus::ports::Persistence;
use types::{crypto_types::Hash32, macros::MacroQc, primitives::Height};

use crate::world::World;

/// Scan ceiling for per-validator macro heights.
const MAX_SCAN_HEIGHT: u64 = 128;

/// Run the `lock_macro` check.
pub fn check(world: &World) -> Result<()> {
    let mut by_height: BTreeMap<Height, (Hash32, Option<MacroQc>)> = BTreeMap::new();
    let mut hashes_seen: BTreeSet<Hash32> = BTreeSet::new();

    for p in &world.persistence {
        for h in 0..MAX_SCAN_HEIGHT {
            let height = Height(h);
            let Some(cp) = p.macro_checkpoint_at(height).ok().flatten() else {
                continue;
            };
            hashes_seen.insert(cp.hash);
            let qc = p.macro_qc_for(&cp.hash).ok().flatten();
            match by_height.get(&height) {
                None => {
                    by_height.insert(height, (cp.hash, qc));
                }
                Some((existing_hash, existing_qc)) => {
                    if *existing_hash != cp.hash {
                        bail!(
                            "lock_macro: validators adopted conflicting MacroCheckpoints at height {height:?}: {existing_hash:?} vs {:?}",
                            cp.hash
                        );
                    }
                    if let (Some(a), Some(b)) = (existing_qc, &qc) {
                        if a != b {
                            bail!(
                                "lock_macro: validators adopted conflicting MacroQcs at height {height:?} for hash {:?}",
                                cp.hash
                            );
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
