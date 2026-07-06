use std::collections::HashMap;
use std::sync::Mutex;

use dag::blob::chunk::{BlobChunk, ChunkPayload};
use dag::blob::commit::blob_id_from_payload;
use dag::blob::custody::CustodyLedger;
use dag::blob::store::{BlobStore, StoreError};
use dag::erasure::{encode_shards, ErasureConfig};
use types::{dag::ChunkRef, primitives::BlobId};

#[test]
fn blob_id_is_deterministic() {
    let payload = b"rollup-batch-v0";
    assert_eq!(blob_id_from_payload(payload), blob_id_from_payload(payload));
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
            .insert((chunk.blob_id, chunk.index()), chunk.clone());
        Ok(())
    }

    fn get_chunk(&self, blob_id: &BlobId, index: u32) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self
            .0
            .lock()
            .expect("lock")
            .get(&(*blob_id, index))
            .map(|c| c.data().to_vec()))
    }

    fn has_chunk(&self, blob_id: &BlobId, index: u32) -> Result<bool, StoreError> {
        Ok(self
            .0
            .lock()
            .expect("lock")
            .contains_key(&(*blob_id, index)))
    }

    fn list_chunk_refs(&self, blob_id: &BlobId) -> Result<Vec<ChunkRef>, StoreError> {
        let map = self.0.lock().expect("lock");
        Ok(map
            .keys()
            .filter(|(id, _)| id == blob_id)
            .map(|(_, index)| ChunkRef {
                blob_id: *blob_id,
                index: *index,
            })
            .collect())
    }
}

#[test]
fn erasure_custody_available_with_k_data_shards_only() {
    let cfg = ErasureConfig::devnet_default();
    let payload = vec![0xEEu8; 100_000];
    let shards = encode_shards(&payload, &cfg).unwrap();
    let blob_id = blob_id_from_payload(&payload);
    let store = MemStore::new();
    let mut ledger = CustodyLedger::default();
    ledger.register_erasure(blob_id, cfg, cfg.n, payload.len() as u64);

    for (index, data) in shards.iter().enumerate().take(usize::try_from(cfg.k).unwrap()) {
        let chunk = BlobChunk {
            blob_id,
            size_bytes: payload.len() as u64,
            payload: ChunkPayload::Erasure {
                index: u32::try_from(index).unwrap(),
                n_shards: cfg.n,
                data: data.clone(),
            },
        };
        store.put_chunk(&chunk).unwrap();
        ledger.note_chunk(&blob_id, chunk.index(), &store);
    }
    assert!(ledger.is_available(&blob_id));
}
