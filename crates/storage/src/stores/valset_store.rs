//! Validator-set snapshots per epoch.

use types::{primitives::Epoch, validator::ValidatorSet};

use crate::{
    columns::ColumnFamily,
    db::Database,
    error::{Error, Result},
    keys,
};

/// Store a validator-set snapshot for `set.epoch`.
pub fn put(db: &Database, set: &ValidatorSet) -> Result<()> {
    let key = keys::epoch(set.epoch);
    let bytes = borsh::to_vec(set).map_err(|e| Error::Codec(e.to_string()))?;
    db.put_raw(ColumnFamily::ValidatorSet, &key, &bytes)
}

/// Fetch the active validator set for `epoch`.
pub fn get(db: &Database, epoch: Epoch) -> Result<Option<ValidatorSet>> {
    let key = keys::epoch(epoch);
    match db.get_raw(ColumnFamily::ValidatorSet, &key)? {
        Some(bytes) => borsh::from_slice(&bytes)
            .map(Some)
            .map_err(|e| Error::Codec(e.to_string())),
        None => Ok(None),
    }
}
