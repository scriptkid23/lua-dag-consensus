//! Rocks-backed [`BlobStore`] and host blob custody task (07b).

mod rocks_store;

use std::sync::Arc;

use anyhow::Result;
use dag::blob::chunk::{split_payload, BlobChunk};
use dag::blob::commit::blob_id_from_payload;
use dag::blob::custody::CustodyLedger;
use dag::blob::store::BlobStore;
use net::gossip::Topic;
use net::gossip_wire::encode_blob_chunk;
use tokio::sync::{mpsc, Mutex};
use types::primitives::BlobId;

use crate::observability::metrics::Metrics;

pub use rocks_store::RocksBlobStore;

/// Shared handle for RPC publish and L1 demo blob attachment.
#[derive(Clone)]
pub struct BlobCustodyHandle {
    store: Arc<dyn BlobStore>,
    ledger: Arc<Mutex<CustodyLedger>>,
    publish_tx: mpsc::Sender<(Topic, Vec<u8>)>,
    chunk_size: u32,
    metrics: Arc<Metrics>,
}

impl BlobCustodyHandle {
    /// Whether all chunks for `blob_id` are stored locally.
    #[must_use]
    pub fn is_available(&self, blob_id: &BlobId) -> bool {
        self.ledger.lock().expect("lock").is_available(blob_id)
    }

    /// Split `payload`, store chunks locally, and gossip each chunk.
    pub async fn publish_payload(&self, payload: Vec<u8>) -> Result<BlobId> {
        let blob_id = blob_id_from_payload(&payload);
        for chunk in split_payload(&payload, self.chunk_size) {
            let (topic, bytes) = encode_blob_chunk(&chunk)?;
            self.publish_tx.send((topic, bytes)).await?;
            self.store.put_chunk(&chunk)?;
            let mut ledger = self.ledger.lock().expect("lock");
            ledger.register_meta(chunk.blob_id, chunk.total_chunks, chunk.size_bytes);
            let _ = ledger.note_chunk(&chunk.blob_id, chunk.index, &*self.store);
            self.metrics.blob_chunks_published.inc();
        }
        Ok(blob_id)
    }

    /// Chunk count for a payload of `size_bytes` at the configured chunk size.
    #[must_use]
    pub fn chunk_count_for(&self, size_bytes: u64) -> u32 {
        dag::blob::chunk::chunk_count(size_bytes, self.chunk_size)
    }
}

/// Ingest gossip chunks and track local custody availability.
pub struct BlobCustody {
    store: Arc<dyn BlobStore>,
    ledger: Arc<Mutex<CustodyLedger>>,
    chunks_rx: mpsc::Receiver<BlobChunk>,
    metrics: Arc<Metrics>,
}

impl BlobCustody {
    /// Spawn the custody ingest loop and return a shared handle.
    pub fn spawn(
        store: Arc<dyn BlobStore>,
        chunks_rx: mpsc::Receiver<BlobChunk>,
        publish_tx: mpsc::Sender<(Topic, Vec<u8>)>,
        chunk_size: u32,
        metrics: Arc<Metrics>,
    ) -> BlobCustodyHandle {
        let ledger = Arc::new(Mutex::new(CustodyLedger::default()));
        let handle = BlobCustodyHandle {
            store: Arc::clone(&store),
            ledger: Arc::clone(&ledger),
            publish_tx,
            chunk_size,
            metrics: Arc::clone(&metrics),
        };
        let custody = Self {
            store,
            ledger,
            chunks_rx,
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
            let mut ledger = self.ledger.lock().expect("lock");
            ledger.register_meta(chunk.blob_id, chunk.total_chunks, chunk.size_bytes);
            if ledger.note_chunk(&chunk.blob_id, chunk.index, &*self.store) {
                self.metrics.blob_available.inc();
            }
        }
    }
}
