//! Rocks-backed [`BlobStore`] and host blob custody task (07b/07c).

mod rocks_store;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use dag::blob::chunk::{erasure_chunks, BlobChunk, ChunkPayload};
use dag::blob::commit::blob_id_from_payload;
use dag::blob::custody::CustodyLedger;
use dag::blob::store::BlobStore;
use dag::erasure::{encode_shards, rs_merkle_commitment, ErasureConfig};
use net::gossip::Topic;
use net::gossip_wire::encode_blob_chunk;
use tokio::sync::mpsc;
use types::{crypto_types::Hash32, dag::{BlobRef, ChunkRef}, primitives::BlobId};

use crate::observability::metrics::Metrics;

pub use rocks_store::RocksBlobStore;

/// Publish + custody configuration (erasure-only).
#[derive(Clone, Debug)]
pub struct BlobCustodyConfig {
    /// RS parameters; every blob is encoded to `n` shards of
    /// `data_shard_size` bytes, max payload `k * data_shard_size`.
    pub erasure: ErasureConfig,
}

/// Shared handle for RPC publish + L1 driver pending-attach drain.
#[derive(Clone)]
pub struct BlobCustodyHandle {
    store: Arc<dyn BlobStore>,
    ledger: Arc<Mutex<CustodyLedger>>,
    publish_tx: mpsc::Sender<(Topic, Vec<u8>)>,
    config: BlobCustodyConfig,
    metrics: Arc<Metrics>,
    pending: Arc<Mutex<VecDeque<BlobRef>>>,
}

impl std::fmt::Debug for BlobCustodyHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlobCustodyHandle")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl BlobCustodyHandle {
    /// Whether all chunks/shards for `blob_id` are stored locally.
    #[must_use]
    pub fn is_available(&self, blob_id: &BlobId) -> bool {
        self.ledger.lock().expect("lock").is_available(blob_id)
    }

    /// Shard count for every published blob (`n`).
    #[must_use]
    pub fn unit_count(&self) -> u32 {
        self.config.erasure.n
    }

    /// Maximum accepted payload size in bytes (`k * data_shard_size`).
    #[must_use]
    pub fn max_payload_bytes(&self) -> u64 {
        self.config.erasure.padded_len() as u64
    }

    /// RS-Merkle commitment carried in `BlobRef`.
    #[must_use]
    pub fn blob_ref_commitment(&self, payload: &[u8]) -> Hash32 {
        let shards = encode_shards(payload, &self.config.erasure).expect("encode shards");
        rs_merkle_commitment(&shards)
    }

    /// List stored chunk refs for a blob.
    pub fn list_chunk_refs(&self, blob_id: &BlobId) -> Result<Vec<ChunkRef>> {
        self.store
            .list_chunk_refs(blob_id)
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Pop every queued `BlobRef` in FIFO order. Drained by the `vertex_cert`
    /// proposer (via `PendingBlobSource`) when building this node's proposal.
    #[must_use]
    pub fn drain_pending(&self) -> Vec<BlobRef> {
        let mut q = self.pending.lock().expect("lock");
        q.drain(..).collect()
    }

    fn enqueue_pending(&self, blob: BlobRef) {
        self.pending.lock().expect("lock").push_back(blob);
    }

    /// Store payload locally and gossip each chunk/shard.
    pub async fn publish_payload(&self, payload: Vec<u8>) -> Result<BlobId> {
        let blob_id = blob_id_from_payload(&payload);
        let size_bytes = u64::try_from(payload.len()).expect("payload fits u64");
        let shards = encode_shards(&payload, &self.config.erasure)?;
        let chunks = erasure_chunks(blob_id, size_bytes, &shards);

        for chunk in chunks {
            let (topic, bytes) = encode_blob_chunk(&chunk)?;
            self.publish_tx.send((topic, bytes)).await?;
            self.store.put_chunk(&chunk)?;
            self.register_chunk_meta(&chunk);
            let mut ledger = self.ledger.lock().expect("lock");
            if ledger.note_chunk(&chunk.blob_id, chunk.index(), &*self.store) {
                self.metrics.blob_available.inc();
            }
            self.metrics.blob_chunks_published.inc();
        }

        self.enqueue_pending(BlobRef {
            blob_id,
            commitment: self.blob_ref_commitment(&payload),
            size_bytes,
        });
        Ok(blob_id)
    }

    fn register_chunk_meta(&self, chunk: &BlobChunk) {
        register_chunk_in_ledger(
            &mut self.ledger.lock().expect("lock"),
            chunk,
            self.config.erasure,
        );
    }
}

/// Ingest gossip chunks and track local custody availability.
pub struct BlobCustody {
    store: Arc<dyn BlobStore>,
    ledger: Arc<Mutex<CustodyLedger>>,
    chunks_rx: mpsc::Receiver<BlobChunk>,
    config: BlobCustodyConfig,
    metrics: Arc<Metrics>,
}

impl BlobCustody {
    /// Spawn the custody ingest loop and return a shared handle.
    pub fn spawn(
        store: Arc<dyn BlobStore>,
        chunks_rx: mpsc::Receiver<BlobChunk>,
        publish_tx: mpsc::Sender<(Topic, Vec<u8>)>,
        config: BlobCustodyConfig,
        metrics: Arc<Metrics>,
    ) -> BlobCustodyHandle {
        let ledger = Arc::new(Mutex::new(CustodyLedger::default()));
        let handle = BlobCustodyHandle {
            store: Arc::clone(&store),
            ledger: Arc::clone(&ledger),
            publish_tx,
            config: config.clone(),
            metrics: Arc::clone(&metrics),
            pending: Arc::new(Mutex::new(VecDeque::new())),
        };
        let custody = Self {
            store,
            ledger,
            chunks_rx,
            config,
            metrics,
        };
        tokio::spawn(async move {
            custody.run().await;
        });
        handle
    }

    async fn run(mut self) {
        while let Some(chunk) = self.chunks_rx.recv().await {
            if self.store.put_chunk(&chunk).is_err() {
                self.metrics.blob_chunk_rejected.inc();
                continue;
            }
            self.metrics.blob_chunks_received.inc();
            {
                let mut ledger = self.ledger.lock().expect("lock");
                register_chunk_in_ledger(&mut ledger, &chunk, self.config.erasure);
            }
            let mut ledger = self.ledger.lock().expect("lock");
            if ledger.note_chunk(&chunk.blob_id, chunk.index(), &*self.store) {
                self.metrics.blob_available.inc();
            }
        }
    }
}

fn register_chunk_in_ledger(
    ledger: &mut CustodyLedger,
    chunk: &BlobChunk,
    erasure: ErasureConfig,
) {
    let ChunkPayload::Erasure { n_shards, .. } = &chunk.payload;
    ledger.register_erasure(chunk.blob_id, erasure, *n_shards, chunk.size_bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::metrics::Metrics;
    use dag::blob::store::BlobStore;
    use std::sync::Arc;

    fn spawn_handle() -> BlobCustodyHandle {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(
            storage::Database::open(&storage::config::StorageConfig {
                path: dir.path().to_path_buf(),
                create_if_missing: true,
                max_total_wal_size_mb: 16,
            })
            .unwrap(),
        );
        let store: Arc<dyn BlobStore> = Arc::new(RocksBlobStore::new(db));
        let (publish_tx, mut publish_rx) = mpsc::channel(256);
        let (_chunks_tx, chunks_rx) = mpsc::channel(64);
        tokio::spawn(async move {
            while publish_rx.recv().await.is_some() {}
        });
        let metrics = Arc::new(Metrics::new().unwrap());
        BlobCustody::spawn(
            store,
            chunks_rx,
            publish_tx,
            BlobCustodyConfig {
                erasure: dag::erasure::ErasureConfig {
                    k: 4,
                    n: 8,
                    data_shard_size: 1024,
                },
            },
            metrics,
        )
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pending_queue_fifo_and_drain() {
        let handle = spawn_handle();
        let pa = vec![0xA1u8; 1500];
        let pb = vec![0xB2u8; 1500];
        let pc = vec![0xC3u8; 1500];
        let id_a = handle.publish_payload(pa.clone()).await.unwrap();
        let id_b = handle.publish_payload(pb.clone()).await.unwrap();
        let id_c = handle.publish_payload(pc.clone()).await.unwrap();

        let drained = handle.drain_pending();
        assert_eq!(drained.len(), 3);
        assert_eq!(drained[0].blob_id, id_a);
        assert_eq!(drained[1].blob_id, id_b);
        assert_eq!(drained[2].blob_id, id_c);
        assert_eq!(drained[0].size_bytes, pa.len() as u64);
        assert_eq!(drained[1].size_bytes, pb.len() as u64);
        assert_eq!(drained[2].size_bytes, pc.len() as u64);

        assert!(handle.drain_pending().is_empty());
    }
}
