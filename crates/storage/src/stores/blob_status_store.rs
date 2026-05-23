//! `blob_id -> BlobStatus` with monotonic lifecycle updates.

use consensus::api::tier::BlobStatus;
use types::primitives::BlobId;

use crate::{
    columns::ColumnFamily,
    db::Database,
    error::{Error, Result},
    keys,
};

fn decode_status(byte: u8) -> Result<BlobStatus> {
    match byte {
        0 => Ok(BlobStatus::Accepted),
        1 => Ok(BlobStatus::SoftConfirmed),
        2 => Ok(BlobStatus::Justified),
        3 => Ok(BlobStatus::Finalized),
        4 => Ok(BlobStatus::EpochFinalized),
        _ => Err(Error::Logic("invalid blob status byte")),
    }
}

/// Fetch stored status for `blob`, if any.
pub fn get(db: &Database, blob: &BlobId) -> Result<Option<BlobStatus>> {
    let key = keys::blob_id(blob);
    let Some(bytes) = db.get_raw(ColumnFamily::BlobStatus, &key)? else {
        return Ok(None);
    };
    if bytes.len() != 1 {
        return Err(Error::Logic("blob_status row has wrong length"));
    }
    decode_status(bytes[0]).map(Some)
}

/// Monotonic upsert: never downgrades an existing tier.
pub fn put_monotonic(db: &Database, blob: &BlobId, status: BlobStatus) -> Result<()> {
    let current = get(db, blob)?;
    let should_write = match current {
        None => true,
        Some(existing) => status > existing,
    };
    if should_write {
        let key = keys::blob_id(blob);
        db.put_raw(ColumnFamily::BlobStatus, &key, &[status as u8])?;
    }
    Ok(())
}
