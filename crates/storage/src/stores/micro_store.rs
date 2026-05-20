//! Micro checkpoints + QCs.

use types::micro::{MicroCheckpoint, MicroQc};

use crate::{
    columns::ColumnFamily,
    db::Database,
    error::{Error, Result},
    keys,
};

/// Store a micro checkpoint keyed by its anchor round.
pub fn put_checkpoint(db: &Database, cp: &MicroCheckpoint) -> Result<()> {
    let key = keys::slot(cp.anchor_round.0);
    let bytes = borsh::to_vec(cp).map_err(|e| Error::Codec(e.to_string()))?;
    db.put_raw(ColumnFamily::MicroCheckpoint, &key, &bytes)
}

/// Fetch by anchor round.
pub fn get_checkpoint(db: &Database, slot: u64) -> Result<Option<MicroCheckpoint>> {
    let key = keys::slot(slot);
    match db.get_raw(ColumnFamily::MicroCheckpoint, &key)? {
        Some(bytes) => borsh::from_slice(&bytes)
            .map(Some)
            .map_err(|e| Error::Codec(e.to_string())),
        None => Ok(None),
    }
}

/// Store a micro QC keyed by its `checkpoint_hash`.
pub fn put_qc(db: &Database, qc: &MicroQc) -> Result<()> {
    let key = keys::hash(&qc.checkpoint_hash);
    let bytes = borsh::to_vec(qc).map_err(|e| Error::Codec(e.to_string()))?;
    db.put_raw(ColumnFamily::MicroQc, &key, &bytes)
}

/// Fetch a micro QC by `checkpoint_hash`.
pub fn get_qc(db: &Database, checkpoint_hash: &types::crypto_types::Hash32) -> Result<Option<MicroQc>> {
    let key = keys::hash(checkpoint_hash);
    match db.get_raw(ColumnFamily::MicroQc, &key)? {
        Some(bytes) => borsh::from_slice(&bytes)
            .map(Some)
            .map_err(|e| Error::Codec(e.to_string())),
        None => Ok(None),
    }
}
