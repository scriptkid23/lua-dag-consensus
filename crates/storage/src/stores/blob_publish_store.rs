//! `blob_id -> PublishRecord` (local publish/attach lifecycle).

use borsh::{BorshDeserialize, BorshSerialize};
use rocksdb::WriteBatch;
use types::{dag::BlobRef, primitives::BlobId};

use crate::{
    columns::ColumnFamily,
    db::Database,
    error::{Error, Result},
    keys,
};

/// Publish lifecycle discriminant (stored as `u8` in `PublishRecord`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishState {
    /// Locally durable, awaiting vertex attach.
    Ready = 0,
    /// Included in a sealed local vertex proposal.
    Attached = 1,
}

impl PublishState {
    fn from_u8(byte: u8) -> Result<Self> {
        match byte {
            0 => Ok(Self::Ready),
            1 => Ok(Self::Attached),
            _ => Err(Error::Logic("invalid publish state byte")),
        }
    }
}

/// Borsh row in `BlobPublish` CF.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct PublishRecord {
    /// [`PublishState`] discriminant.
    pub state: u8,
    /// Authoritative attach metadata.
    pub blob_ref: BlobRef,
}

/// Append a `Ready` publish record to an existing `WriteBatch`.
pub fn put_ready_batch(
    batch: &mut WriteBatch,
    db: &Database,
    blob_id: &BlobId,
    record: &PublishRecord,
) -> Result<()> {
    if record.state != PublishState::Ready as u8 {
        return Err(Error::Logic("put_ready_batch requires Ready state"));
    }
    let key = keys::blob_id(blob_id);
    let bytes = borsh::to_vec(record).map_err(|e| Error::Codec(e.to_string()))?;
    let h = db.cf(ColumnFamily::BlobPublish)?;
    batch.put_cf(h, key, bytes);
    Ok(())
}

/// Transition `Ready -> Attached`. `Attached -> Attached` is idempotent no-op.
pub fn put_attached(db: &Database, blob_id: &BlobId) -> Result<()> {
    let key = keys::blob_id(blob_id);
    let Some(bytes) = db.get_raw(ColumnFamily::BlobPublish, &key)? else {
        return Err(Error::Logic("mark_attached: publish record missing"));
    };
    let mut record: PublishRecord =
        borsh::from_slice(&bytes).map_err(|e| Error::Codec(e.to_string()))?;
    let state = PublishState::from_u8(record.state)?;
    match state {
        PublishState::Attached => return Ok(()),
        PublishState::Ready => {
            record.state = PublishState::Attached as u8;
            let out = borsh::to_vec(&record).map_err(|e| Error::Codec(e.to_string()))?;
            db.put_raw(ColumnFamily::BlobPublish, &key, &out)
        }
    }
}

/// Fetch publish record for `blob_id`, if any.
pub fn get(db: &Database, blob_id: &BlobId) -> Result<Option<PublishRecord>> {
    let key = keys::blob_id(blob_id);
    let Some(bytes) = db.get_raw(ColumnFamily::BlobPublish, &key)? else {
        return Ok(None);
    };
    let record: PublishRecord =
        borsh::from_slice(&bytes).map_err(|e| Error::Codec(e.to_string()))?;
    Ok(Some(record))
}

/// Whether `blob_id` is in `Attached` state.
pub fn is_attached(db: &Database, blob_id: &BlobId) -> Result<bool> {
    let Some(record) = get(db, blob_id)? else {
        return Ok(false);
    };
    Ok(PublishState::from_u8(record.state)? == PublishState::Attached)
}

/// All `BlobRef` rows currently in `Ready` state (boot recovery).
pub fn scan_ready(db: &Database) -> Result<Vec<BlobRef>> {
    let mut out = Vec::new();
    for item in db.scan_cf(ColumnFamily::BlobPublish) {
        let (_, value) = item?;
        let record: PublishRecord =
            borsh::from_slice(&value).map_err(|e| Error::Codec(e.to_string()))?;
        if PublishState::from_u8(record.state)? == PublishState::Ready {
            out.push(record.blob_ref);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::StorageConfig, wal};
    use tempfile::tempdir;
    use types::crypto_types::Hash32;

    fn test_db() -> (tempfile::TempDir, Database) {
        let dir = tempdir().unwrap();
        let db = Database::open(&StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 16,
        })
        .unwrap();
        (dir, db)
    }

    fn sample_ref(blob_id: BlobId) -> BlobRef {
        BlobRef {
            blob_id,
            commitment: Hash32([0xAB; 32]),
            size_bytes: 4096,
        }
    }

    #[test]
    fn blob_publish_store_roundtrip() {
        let (_dir, db) = test_db();
        let blob_id = BlobId([0x01; 32]);
        let blob_ref = sample_ref(blob_id);
        let record = PublishRecord {
            state: PublishState::Ready as u8,
            blob_ref,
        };
        let mut batch = rocksdb::WriteBatch::default();
        put_ready_batch(&mut batch, &db, &blob_id, &record).unwrap();
        wal::apply(&db, batch).unwrap();

        let got = get(&db, &blob_id).unwrap().expect("record");
        assert_eq!(got, record);
        assert!(!is_attached(&db, &blob_id).unwrap());

        put_attached(&db, &blob_id).unwrap();
        assert!(is_attached(&db, &blob_id).unwrap());
        let attached = get(&db, &blob_id).unwrap().expect("record");
        assert_eq!(attached.state, PublishState::Attached as u8);

        // Attached -> Attached is idempotent no-op.
        put_attached(&db, &blob_id).unwrap();
        assert!(is_attached(&db, &blob_id).unwrap());
    }

    #[test]
    fn put_batch_atomic_with_wal() {
        use crate::stores::blob_chunk_store;

        let (_dir, db) = test_db();
        let blob_id = BlobId([0x02; 32]);
        let blob_ref = sample_ref(blob_id);
        let record = PublishRecord {
            state: PublishState::Ready as u8,
            blob_ref,
        };
        let mut batch = rocksdb::WriteBatch::default();
        blob_chunk_store::put_batch(&mut batch, &db, &blob_id, 0, 4, 4096, b"chunk0").unwrap();
        blob_chunk_store::put_batch(&mut batch, &db, &blob_id, 1, 4, 4096, b"chunk1").unwrap();
        put_ready_batch(&mut batch, &db, &blob_id, &record).unwrap();
        wal::apply(&db, batch).unwrap();

        assert!(blob_chunk_store::has(&db, &blob_id, 0).unwrap());
        assert!(blob_chunk_store::has(&db, &blob_id, 1).unwrap());
        assert_eq!(get(&db, &blob_id).unwrap().unwrap().blob_ref, blob_ref);
    }

    #[test]
    fn writebatch_rollback() {
        use crate::stores::blob_chunk_store;

        let (_dir, db) = test_db();
        let blob_id = BlobId([0x03; 32]);
        let blob_ref = sample_ref(blob_id);
        let record = PublishRecord {
            state: PublishState::Ready as u8,
            blob_ref,
        };
        let mut batch = rocksdb::WriteBatch::default();
        blob_chunk_store::put_batch(&mut batch, &db, &blob_id, 0, 2, 1024, b"data").unwrap();
        put_ready_batch(&mut batch, &db, &blob_id, &record).unwrap();
        // Drop batch without apply — nothing persisted.
        drop(batch);

        assert!(!blob_chunk_store::has(&db, &blob_id, 0).unwrap());
        assert!(get(&db, &blob_id).unwrap().is_none());
    }

    #[test]
    fn scan_ready_skips_attached() {
        let (_dir, db) = test_db();
        let ready_id = BlobId([0x04; 32]);
        let attached_id = BlobId([0x05; 32]);
        for (id, mark_attached) in [(ready_id, false), (attached_id, true)] {
            let record = PublishRecord {
                state: PublishState::Ready as u8,
                blob_ref: sample_ref(id),
            };
            let mut batch = rocksdb::WriteBatch::default();
            put_ready_batch(&mut batch, &db, &id, &record).unwrap();
            wal::apply(&db, batch).unwrap();
            if mark_attached {
                put_attached(&db, &id).unwrap();
            }
        }
        let ready = scan_ready(&db).unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].blob_id, ready_id);
    }
}
