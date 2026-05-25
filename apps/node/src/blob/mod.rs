//! Rocks-backed [`BlobStore`] and host blob custody task (07b/07c).

mod rocks_store;

use std::sync::Arc;

use anyhow::Result;
use dag::blob::chunk::{erasure_chunks, split_payload, BlobChunk, ChunkPayload};
use dag::blob::commit::{blob_commitment, blob_id_from_payload};
use dag::blob::custody::CustodyLedger;
use dag::blob::store::BlobStore;
use dag::erasure::{encode_shards, rs_merkle_commitment, ErasureConfig};
use net::gossip::Topic;
use net::gossip_wire::encode_blob_chunk;
use tokio::sync::{mpsc, Mutex};
use types::{crypto_types::Hash32, dag::ChunkRef, primitives::BlobId};

use crate::observability::metrics::Metrics;

pub use rocks_store::RocksBlobStore;

/// Publish + custody configuration.
#[derive(Clone, Debug)]
pub struct BlobCustodyConfig {
    /// Sequential chunk size when erasure is disabled (07b).
    pub chunk_size: u32,
    /// RS parameters when erasure is enabled (07c).
    pub erasure: Option<ErasureConfig>,
}

/// Shared handle for RPC publish and L1 demo blob attachment.
#[derive(Clone)]
pub struct BlobCustodyHandle {
    store: Arc<dyn BlobStore>,
    ledger: Arc<Mutex<CustodyLedger>>,
    publish_tx: mpsc::Sender<(Topic, Vec<u8>)>,
    config: BlobCustodyConfig,
    metrics: Arc<Metrics>,
}

impl BlobCustodyHandle {
    /// Whether all chunks/shards for `blob_id` are stored locally.
    #[must_use]
    pub fn is_available(&self, blob_id: &BlobId) -> bool {
        self.ledger.lock().expect("lock").is_available(blob_id)
    }

    /// Chunk/shard count for a payload at the configured mode.
    #[must_use]
    pub fn unit_count_for(&self, size_bytes: u64) -> u32 {
        if let Some(cfg) = &self.config.erasure {
            cfg.n
        } else {
            dag::blob::chunk::chunk_count(size_bytes, self.config.chunk_size)
        }
    }

    /// Commitment for `BlobRef` under the active mode.
    #[must_use]
    pub fn blob_ref_commitment(&self, payload: &[u8]) -> Hash32 {
        if let Some(cfg) = &self.config.erasure {
            let shards = encode_shards(payload, cfg).expect("encode shards");
            rs_merkle_commitment(&shards)
        } else {
            blob_commitment(payload)
        }
    }

    /// List stored chunk refs for a blob.
    pub fn list_chunk_refs(&self, blob_id: &BlobId) -> Result<Vec<ChunkRef>> {
        self.store
            .list_chunk_refs(blob_id)
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Store payload locally and gossip each chunk/shard.
    pub async fn publish_payload(&self, payload: Vec<u8>) -> Result<BlobId> {
        let blob_id = blob_id_from_payload(&payload);
        let size_bytes = u64::try_from(payload.len()).expect("payload fits u64");
        let chunks = if let Some(cfg) = &self.config.erasure {
            let shards = encode_shards(&payload, cfg)?;
            erasure_chunks(blob_id, size_bytes, &shards)
        } else {
            split_payload(&payload, self.config.chunk_size)
        };

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
    erasure: Option<ErasureConfig>,
) {
    match &chunk.payload {
        ChunkPayload::Sequential { total_chunks, .. } => {
            ledger.register_sequential(chunk.blob_id, *total_chunks, chunk.size_bytes);
        }
        ChunkPayload::Erasure { n_shards, .. } => {
            if let Some(cfg) = erasure {
                ledger.register_erasure(chunk.blob_id, cfg, *n_shards, chunk.size_bytes);
            }
        }
    }
}
