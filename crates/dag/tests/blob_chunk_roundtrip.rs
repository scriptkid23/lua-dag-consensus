use std::collections::HashMap;
use std::sync::Mutex;

use dag::blob::chunk::{chunk_count, split_payload, BlobChunk};
use dag::blob::commit::{blob_commitment, blob_id_from_payload};
use dag::blob::custody::CustodyLedger;
use dag::blob::store::{BlobStore, StoreError};
use types::{crypto_types::Hash32, primitives::BlobId};

#[test]
fn blob_id_and_commitment_are_deterministic() {
    let payload = b"rollup-batch-v0";
    let id1 = blob_id_from_payload(payload);
    let id2 = blob_id_from_payload(payload);
    assert_eq!(id1, id2);
    assert_ne!(blob_commitment(payload), Hash32(id1.0));
}

#[test]
fn split_100k_payload_with_64k_chunks() {
    let payload = vec![0xABu8; 100_000];
    let chunks = split_payload(&payload, 65_536);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].index, 0);
    assert_eq!(chunks[1].index, 1);
    assert_eq!(chunks[0].data.len(), 65_536);
    assert_eq!(chunks[1].data.len(), 100_000 - 65_536);
    let rebuilt: Vec<u8> = chunks.iter().flat_map(|c| c.data.clone()).collect();
    assert_eq!(rebuilt, payload);
}

#[test]
fn chunk_count_ceil_div() {
    assert_eq!(chunk_count(1, 64), 1);
    assert_eq!(chunk_count(65_536, 65_536), 1);
    assert_eq!(chunk_count(65_537, 65_536), 2);
}

struct MemStore(Mutex<HashMap<(BlobId, u32), BlobChunk>>);

impl MemStore {
    fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

impl BlobStore for MemStore {
    fn put_chunk(&self, chunk: &BlobChunk) -> Result<(), StoreError> {
        self.0
            .lock()
            .expect("lock")
            .insert((chunk.blob_id, chunk.index), chunk.clone());
        Ok(())
    }

    fn get_chunk(&self, blob_id: &BlobId, index: u32) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self
            .0
            .lock()
            .expect("lock")
            .get(&(*blob_id, index))
            .map(|c| c.data.clone()))
    }

    fn has_chunk(&self, blob_id: &BlobId, index: u32) -> Result<bool, StoreError> {
        Ok(self
            .0
            .lock()
            .expect("lock")
            .contains_key(&(*blob_id, index)))
    }
}

#[test]
fn custody_marks_blob_available_when_all_chunks_present() {
    let payload = vec![0xCDu8; 100_000];
    let chunks = split_payload(&payload, 65_536);
    let store = MemStore::new();
    let mut ledger = CustodyLedger::default();
    let blob_id = blob_id_from_payload(&payload);

    ledger.register_meta(blob_id, chunks[0].total_chunks, chunks[0].size_bytes);
    store.put_chunk(&chunks[0]).unwrap();
    assert!(!ledger.note_chunk(&blob_id, 0, &store));
    assert!(!ledger.is_available(&blob_id));

    store.put_chunk(&chunks[1]).unwrap();
    assert!(ledger.note_chunk(&blob_id, 1, &store));
    assert!(ledger.is_available(&blob_id));
}
