//! No two distinct `MicroQc` values share the same `checkpoint_hash`,
//! and no two distinct `MacroQc` values share the same `checkpoint_hash`.

use std::collections::HashMap;

use anyhow::{Result, bail};

use crate::world::World;

/// Run the safety check.
pub fn check(world: &World) -> Result<()> {
    let mut by_micro: HashMap<types::crypto_types::Hash32, types::micro::MicroQc> = HashMap::new();
    for p in &world.persistence {
        for qc in p.all_micro_qcs() {
            if let Some(existing) = by_micro.get(&qc.checkpoint_hash) {
                if existing != &qc {
                    bail!("conflicting MicroQc for checkpoint {:?}", qc.checkpoint_hash);
                }
            } else {
                by_micro.insert(qc.checkpoint_hash, qc);
            }
        }
    }

    let mut macro_by_hash: HashMap<types::crypto_types::Hash32, types::macros::MacroQc> = HashMap::new();
    for p in &world.persistence {
        for qc in p.all_macro_qcs() {
            if let Some(existing) = macro_by_hash.get(&qc.checkpoint_hash) {
                if existing != &qc {
                    bail!("conflicting MacroQc for checkpoint {:?}", qc.checkpoint_hash);
                }
            } else {
                macro_by_hash.insert(qc.checkpoint_hash, qc);
            }
        }
    }
    Ok(())
}
