//! In-memory `Persistence` impl for the simulator.

use std::{collections::HashMap, sync::RwLock};

use consensus::ports::persistence::Persistence;
use types::{
    crypto_types::Hash32,
    macros::{MacroCheckpoint, MacroQc},
    micro::MicroQc,
    primitives::Height,
    slashing::SlashEvidence,
};

/// In-memory storage.
#[derive(Debug, Default)]
pub struct VirtualPersistence {
    micro_qcs: RwLock<HashMap<Hash32, MicroQc>>,
    macro_cps: RwLock<HashMap<Height, MacroCheckpoint>>,
    macro_qcs: RwLock<HashMap<Hash32, MacroQc>>,
    slash_log: RwLock<Vec<SlashEvidence>>,
}

impl VirtualPersistence {
    /// Construct empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of slashing entries recorded so far (test helper).
    #[must_use]
    pub fn slash_count(&self) -> usize {
        self.slash_log.read().unwrap().len()
    }

    /// True if at least one micro QC is stored (checker helper).
    #[must_use]
    pub fn any_micro_qc(&self) -> bool {
        !self.micro_qcs.read().unwrap().is_empty()
    }

    /// Snapshot all stored micro QCs (checker helper).
    #[must_use]
    pub fn all_micro_qcs(&self) -> Vec<MicroQc> {
        self.micro_qcs.read().unwrap().values().cloned().collect()
    }
}

impl Persistence for VirtualPersistence {
    fn store_micro_qc(&self, qc: &MicroQc) -> consensus::Result<()> {
        self.micro_qcs
            .write()
            .unwrap()
            .insert(qc.checkpoint_hash, qc.clone());
        Ok(())
    }

    fn micro_qc_for(&self, checkpoint_hash: &Hash32) -> consensus::Result<Option<MicroQc>> {
        Ok(self.micro_qcs.read().unwrap().get(checkpoint_hash).cloned())
    }

    fn store_macro_checkpoint(&self, cp: &MacroCheckpoint) -> consensus::Result<()> {
        self.macro_cps
            .write()
            .unwrap()
            .insert(cp.height, cp.clone());
        Ok(())
    }

    fn store_macro_qc(&self, qc: &MacroQc) -> consensus::Result<()> {
        self.macro_qcs
            .write()
            .unwrap()
            .insert(qc.checkpoint_hash, qc.clone());
        Ok(())
    }

    fn append_slash_evidence(&self, ev: &SlashEvidence) -> consensus::Result<()> {
        self.slash_log.write().unwrap().push(ev.clone());
        Ok(())
    }

    fn macro_checkpoint_at(&self, height: Height) -> consensus::Result<Option<MacroCheckpoint>> {
        Ok(self.macro_cps.read().unwrap().get(&height).cloned())
    }

    fn macro_qc_for(&self, checkpoint_hash: &Hash32) -> consensus::Result<Option<MacroQc>> {
        Ok(self.macro_qcs.read().unwrap().get(checkpoint_hash).cloned())
    }
}
