//! Per-validator vote history. Surround detector consumes this.

use consensus::macro_fin::vote_book::VoteRecord;
use types::{
    crypto_types::Hash32,
    primitives::{Epoch, ValidatorId},
};

use crate::{
    columns::ColumnFamily,
    db::Database,
    error::{Error, Result},
    keys,
};

const VOTE_RECORD_LEN: usize = 8 + 8 + 32;

/// Store a vote record keyed by `(validator, target_epoch)`.
pub fn put(db: &Database, validator: &ValidatorId, record: &VoteRecord) -> Result<()> {
    let key = keys::votebook(validator, record.target);
    // VoteRecord does not derive Borsh — encode the three fields manually.
    let mut bytes = Vec::with_capacity(VOTE_RECORD_LEN);
    bytes.extend_from_slice(&record.source.0.to_be_bytes());
    bytes.extend_from_slice(&record.target.0.to_be_bytes());
    bytes.extend_from_slice(record.checkpoint.as_bytes());
    db.put_raw(ColumnFamily::VoteBook, &key, &bytes)
}

/// Fetch a vote at `target_epoch` for `validator`.
pub fn get(
    db: &Database,
    validator: &ValidatorId,
    target_epoch: Epoch,
) -> Result<Option<VoteRecord>> {
    let key = keys::votebook(validator, target_epoch);
    let Some(bytes) = db.get_raw(ColumnFamily::VoteBook, &key)? else {
        return Ok(None);
    };
    if bytes.len() != VOTE_RECORD_LEN {
        return Err(Error::Logic("vote_book row has wrong length"));
    }
    let source = u64::from_be_bytes(bytes[..8].try_into().unwrap());
    let target = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
    let mut checkpoint = [0u8; 32];
    checkpoint.copy_from_slice(&bytes[16..48]);
    Ok(Some(VoteRecord {
        source: Epoch(source),
        target: Epoch(target),
        checkpoint: Hash32(checkpoint),
    }))
}
