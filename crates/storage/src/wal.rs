//! Write-ahead-log helpers for atomic batch writes.

use rocksdb::WriteBatch;

use crate::{columns::ColumnFamily, db::Database, error::Result};

/// Fresh empty write batch.
pub fn new_batch() -> WriteBatch {
    WriteBatch::default()
}

/// Apply a `WriteBatch` atomically. Wrapping `rocksdb::WriteBatch` lets
/// us swap in pre-flush hooks later without touching call sites.
pub fn apply(db: &Database, batch: WriteBatch) -> Result<()> {
    db.inner().write(batch)?;
    Ok(())
}

/// Convenience builder: writes one key/value pair under a CF in a fresh
/// batch (useful in tests).
pub fn put_one(db: &Database, cf: ColumnFamily, key: &[u8], value: &[u8]) -> Result<()> {
    let mut batch = WriteBatch::default();
    let h = db.cf(cf)?;
    batch.put_cf(h, key, value);
    apply(db, batch)
}
