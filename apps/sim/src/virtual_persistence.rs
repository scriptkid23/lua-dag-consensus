//! In-memory `Persistence` impl for the simulator.

use std::{
    collections::{BTreeMap, HashMap},
    sync::RwLock,
};

use consensus::{api::tier::BlobStatus, ports::persistence::Persistence};
use types::{
    crypto_types::Hash32,
    macros::{MacroCheckpoint, MacroQc},
    micro::MicroQc,
    primitives::{BlobId, Height},
    slashing::SlashEvidence,
};

/// In-memory storage.
#[derive(Debug, Default)]
pub struct VirtualPersistence {
    micro_qcs: RwLock<HashMap<Hash32, MicroQc>>,
    macro_cps: RwLock<HashMap<Height, MacroCheckpoint>>,
    macro_qcs: RwLock<HashMap<Hash32, MacroQc>>,
    slash_log: RwLock<Vec<SlashEvidence>>,
    blob_status: RwLock<BTreeMap<BlobId, BlobStatus>>,
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

    /// True if at least one macro QC is stored (checker helper).
    #[must_use]
    pub fn any_macro_qc(&self) -> bool {
        !self.macro_qcs.read().unwrap().is_empty()
    }

    /// Snapshot all stored macro QCs (checker helper).
    #[must_use]
    pub fn all_macro_qcs(&self) -> Vec<MacroQc> {
        self.macro_qcs.read().unwrap().values().cloned().collect()
    }

    /// Look up a blob's status.
    #[must_use]
    pub fn blob_status(&self, blob: &BlobId) -> Option<BlobStatus> {
        self.blob_status.read().unwrap().get(blob).copied()
    }

    /// Monotonic update: never downgrades.
    pub fn update_blob_status(&self, blob: BlobId, status: BlobStatus) {
        let mut map = self.blob_status.write().unwrap();
        let entry = map.entry(blob).or_insert(status);
        if status > *entry {
            *entry = status;
        }
    }

    /// Count blobs at `BlobStatus::Finalized`.
    #[must_use]
    pub fn finalized_count(&self) -> usize {
        self.blob_status
            .read()
            .unwrap()
            .values()
            .filter(|s| **s == BlobStatus::Finalized)
            .count()
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

#[cfg(test)]
mod tests {
    use super::*;
    use consensus::api::tier::BlobStatus;
    use types::primitives::BlobId;

    #[test]
    fn blob_status_is_monotonic_and_readable() {
        let p = VirtualPersistence::new();
        let blob = BlobId([1; 32]);
        assert_eq!(p.blob_status(&blob), None);
        p.update_blob_status(blob, BlobStatus::SoftConfirmed);
        assert_eq!(p.blob_status(&blob), Some(BlobStatus::SoftConfirmed));
        p.update_blob_status(blob, BlobStatus::Justified);
        assert_eq!(p.blob_status(&blob), Some(BlobStatus::Justified));
        p.update_blob_status(blob, BlobStatus::SoftConfirmed);
        assert_eq!(p.blob_status(&blob), Some(BlobStatus::Justified));
    }

    #[test]
    fn finalized_count_returns_only_finalized() {
        let p = VirtualPersistence::new();
        p.update_blob_status(BlobId([1; 32]), BlobStatus::Finalized);
        p.update_blob_status(BlobId([2; 32]), BlobStatus::Justified);
        p.update_blob_status(BlobId([3; 32]), BlobStatus::Finalized);
        assert_eq!(p.finalized_count(), 2);
    }
}
