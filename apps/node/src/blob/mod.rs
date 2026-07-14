//! Rocks-backed [`BlobStore`] and host blob custody task (07b/07c).

mod rocks_store;

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use dag::blob::chunk::{erasure_chunks, BlobChunk, ChunkPayload};
use dag::blob::commit::blob_id_from_payload;
use dag::blob::custody::CustodyLedger;
use dag::blob::store::BlobStore;
use dag::erasure::{encode_shards, rs_merkle_commitment, ErasureConfig};
use net::gossip::Topic;
use net::gossip_wire::encode_blob_chunk;
use storage::{columns::ColumnFamily, stores::blob_publish_store};
use tokio::sync::mpsc;
use tracing::{debug, warn};
use types::{
    crypto_types::Hash32,
    dag::{BlobRef, ChunkRef},
    primitives::BlobId,
};

use crate::observability::metrics::Metrics;

pub use rocks_store::{PublishRecord, PublishState, RocksBlobStore};

/// Publish + custody configuration (erasure-only).
#[derive(Clone, Debug)]
pub struct BlobCustodyConfig {
    /// RS parameters; every blob is encoded to `n` shards of
    /// `data_shard_size` bytes, max payload `k * data_shard_size`.
    pub erasure: ErasureConfig,
}

/// FIFO pending queue with idempotent enqueue dedup.
#[derive(Debug, Default)]
struct PendingQueue {
    queue: VecDeque<BlobRef>,
    ids: HashSet<BlobId>,
}

impl PendingQueue {
    fn enqueue(&mut self, blob_ref: BlobRef) -> bool {
        if !self.ids.insert(blob_ref.blob_id) {
            return false;
        }
        self.queue.push_back(blob_ref);
        true
    }

    fn drain(&mut self) -> Vec<BlobRef> {
        let drained: Vec<_> = self.queue.drain(..).collect();
        for b in &drained {
            self.ids.remove(&b.blob_id);
        }
        drained
    }
}

/// Shared handle for RPC publish + L1 driver pending-attach drain.
#[derive(Clone)]
pub struct BlobCustodyHandle {
    store: Arc<RocksBlobStore>,
    ledger: Arc<Mutex<CustodyLedger>>,
    publish_tx: mpsc::Sender<(Topic, Vec<u8>)>,
    config: BlobCustodyConfig,
    metrics: Arc<Metrics>,
    pending: Arc<Mutex<PendingQueue>>,
    boot_sync_done: Arc<AtomicBool>,
}

impl std::fmt::Debug for BlobCustodyHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlobCustodyHandle")
            .field("config", &self.config)
            .field(
                "boot_sync_done",
                &self.boot_sync_done.load(Ordering::Acquire),
            )
            .finish_non_exhaustive()
    }
}

impl BlobCustodyHandle {
    /// Whether boot recovery finished (propose/publish gate).
    #[must_use]
    pub fn boot_sync_done(&self) -> bool {
        self.boot_sync_done.load(Ordering::Acquire)
    }

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
            .map_err(|e| anyhow!(e))
    }

    /// Pop queued `BlobRef`s in FIFO order, skipping already-attached blobs.
    #[must_use]
    pub fn drain_pending(&self) -> Vec<BlobRef> {
        if !self.boot_sync_done.load(Ordering::Acquire) {
            return Vec::new();
        }
        let drained = {
            let mut pending = self.pending.lock().expect("lock");
            pending.drain()
        };
        drained
            .into_iter()
            .filter(|b| match self.store.is_attached(&b.blob_id) {
                Ok(true) => {
                    debug!(target: "blob", blob_id = ?b.blob_id, "skip drain: already Attached");
                    false
                }
                Ok(false) => true,
                Err(e) => {
                    warn!(target: "blob", blob_id = ?b.blob_id, "drain re-check failed: {e}");
                    true
                }
            })
            .collect()
    }

    /// Mark blob as attached after local vertex seal+sign.
    pub fn mark_attached(&self, blob_id: BlobId) -> Result<()> {
        match self.store.mark_attached(&blob_id) {
            Ok(()) => Ok(()),
            Err(e) => {
                warn!(target: "blob", ?blob_id, "mark_attached failed: {e}");
                self.metrics.blob_mark_attached_fail_total.inc();
                Err(anyhow!(e.to_string()))
            }
        }
    }

    fn enqueue_pending(&self, blob: BlobRef, from_boot: bool) {
        let mut pending = self.pending.lock().expect("lock");
        if pending.enqueue(blob) && from_boot {
            self.metrics.blob_boot_reenqueue_total.inc();
        }
    }

    /// Store payload locally and gossip each chunk/shard.
    pub async fn publish_payload(&self, payload: Vec<u8>) -> Result<BlobId> {
        if !self.boot_sync_done.load(Ordering::Acquire) {
            return Err(anyhow!("boot recovery not complete"));
        }
        let blob_id = blob_id_from_payload(&payload);
        let size_bytes = u64::try_from(payload.len()).expect("payload fits u64");
        let shards = encode_shards(&payload, &self.config.erasure)?;
        let chunks = erasure_chunks(blob_id, size_bytes, &shards);
        let blob_ref = BlobRef {
            blob_id,
            commitment: self.blob_ref_commitment(&payload),
            size_bytes,
        };
        let record = PublishRecord {
            state: PublishState::Ready as u8,
            blob_ref,
        };

        self.store
            .publish_blob_atomic(&chunks, record)
            .map_err(|e| anyhow!(e.to_string()))?;
        self.metrics.blob_publish_atomic_total.inc();
        self.enqueue_pending(blob_ref, false);

        for chunk in &chunks {
            register_chunk_in_ledger(
                &mut self.ledger.lock().expect("lock"),
                chunk,
                self.config.erasure,
            );
            let mut ledger = self.ledger.lock().expect("lock");
            if ledger.note_chunk(&chunk.blob_id, chunk.index(), &*self.store) {
                self.metrics.blob_available.inc();
            }
        }

        for chunk in chunks {
            let (topic, bytes) = encode_blob_chunk(&chunk)?;
            if self.publish_tx.send((topic, bytes)).await.is_err() {
                warn!(target: "blob", ?blob_id, "gossip send failed; blob durable locally");
            }
            self.metrics.blob_chunks_published.inc();
        }

        Ok(blob_id)
    }
}

/// Ingest gossip chunks and track local custody availability.
pub struct BlobCustody {
    store: Arc<RocksBlobStore>,
    ledger: Arc<Mutex<CustodyLedger>>,
    chunks_rx: mpsc::Receiver<BlobChunk>,
    config: BlobCustodyConfig,
    metrics: Arc<Metrics>,
}

impl BlobCustody {
    /// Spawn the custody ingest loop and return a shared handle.
    pub fn spawn(
        store: Arc<RocksBlobStore>,
        chunks_rx: mpsc::Receiver<BlobChunk>,
        publish_tx: mpsc::Sender<(Topic, Vec<u8>)>,
        config: BlobCustodyConfig,
        metrics: Arc<Metrics>,
    ) -> BlobCustodyHandle {
        let ledger = Arc::new(Mutex::new(CustodyLedger::default()));
        let pending = Arc::new(Mutex::new(PendingQueue::default()));
        let boot_sync_done = Arc::new(AtomicBool::new(false));
        let handle = BlobCustodyHandle {
            store: Arc::clone(&store),
            ledger: Arc::clone(&ledger),
            publish_tx,
            config: config.clone(),
            metrics: Arc::clone(&metrics),
            pending: Arc::clone(&pending),
            boot_sync_done: Arc::clone(&boot_sync_done),
        };
        run_boot_recovery(&handle, &store, &config, &metrics);
        boot_sync_done.store(true, Ordering::Release);
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

fn run_boot_recovery(
    handle: &BlobCustodyHandle,
    store: &RocksBlobStore,
    config: &BlobCustodyConfig,
    metrics: &Metrics,
) {
    let ready = match store.scan_ready_blobs() {
        Ok(r) => r,
        Err(e) => {
            warn!(target: "blob", "boot scan_ready failed: {e}");
            return;
        }
    };
    for blob_ref in ready {
        if !validate_chunks(store, &blob_ref.blob_id, config.erasure.n) {
            warn!(
                target: "blob",
                blob_id = ?blob_ref.blob_id,
                "Ready record missing chunks; skipping re-enqueue"
            );
            metrics.blob_ready_without_chunks_total.inc();
            continue;
        }
        rehydrate_ledger(handle, &blob_ref, config);
        handle.enqueue_pending(blob_ref, true);
    }
    scan_orphan_chunks(store, metrics);
}

fn validate_chunks(store: &RocksBlobStore, blob_id: &BlobId, n: u32) -> bool {
    for index in 0..n {
        match store.has_chunk(blob_id, index) {
            Ok(true) => {}
            Ok(false) | Err(_) => return false,
        }
    }
    true
}

fn rehydrate_ledger(handle: &BlobCustodyHandle, blob_ref: &BlobRef, config: &BlobCustodyConfig) {
    let mut ledger = handle.ledger.lock().expect("lock");
    ledger.register_erasure(
        blob_ref.blob_id,
        config.erasure,
        config.erasure.n,
        blob_ref.size_bytes,
    );
    for index in 0..config.erasure.n {
        if store_has_chunk(&handle.store, &blob_ref.blob_id, index) {
            if ledger.note_chunk_present(&blob_ref.blob_id, index) {
                handle.metrics.blob_available.inc();
            }
        }
    }
}

fn store_has_chunk(store: &RocksBlobStore, blob_id: &BlobId, index: u32) -> bool {
    store.has_chunk(blob_id, index).unwrap_or(false)
}

fn scan_orphan_chunks(store: &RocksBlobStore, metrics: &Metrics) {
    let db = store.db();
    let mut seen_orphans: HashSet<BlobId> = HashSet::new();
    for item in db.scan_cf(ColumnFamily::BlobChunk) {
        let Ok((key, _)) = item else {
            continue;
        };
        if key.len() < 32 {
            continue;
        }
        let mut id_bytes = [0u8; 32];
        id_bytes.copy_from_slice(&key[..32]);
        let blob_id = BlobId(id_bytes);
        if seen_orphans.contains(&blob_id) {
            continue;
        }
        match blob_publish_store::get(db, &blob_id) {
            Ok(None) => {
                seen_orphans.insert(blob_id);
                warn!(target: "blob", ?blob_id, "orphan chunk without PublishRecord");
                metrics.blob_orphan_chunk_total.inc();
            }
            Ok(Some(_)) | Err(_) => {}
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
    use std::sync::Arc;
    use storage::config::StorageConfig;
    use types::crypto_types::Hash32;

    struct TestEnv {
        _dir: tempfile::TempDir,
        db: Arc<storage::Database>,
        store: Arc<RocksBlobStore>,
    }

    impl BlobCustodyHandle {
        fn test_enqueue(&self, blob: BlobRef, from_boot: bool) {
            self.enqueue_pending(blob, from_boot);
        }
    }

    impl TestEnv {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let db = Arc::new(
                storage::Database::open(&StorageConfig {
                    path: dir.path().to_path_buf(),
                    create_if_missing: true,
                    max_total_wal_size_mb: 16,
                })
                .unwrap(),
            );
            let store = Arc::new(RocksBlobStore::new(Arc::clone(&db)));
            Self {
                _dir: dir,
                db,
                store,
            }
        }

        fn spawn_handle(&self) -> (BlobCustodyHandle, Arc<Metrics>) {
            let (publish_tx, mut publish_rx) = mpsc::channel(256);
            tokio::spawn(async move {
                while publish_rx.recv().await.is_some() {}
            });
            let (_chunks_tx, chunks_rx) = mpsc::channel(64);
            let metrics = Arc::new(Metrics::new().unwrap());
            let handle = BlobCustody::spawn(
                Arc::clone(&self.store),
                chunks_rx,
                publish_tx,
                BlobCustodyConfig {
                    erasure: ErasureConfig {
                        k: 4,
                        n: 8,
                        data_shard_size: 1024,
                    },
                },
                Arc::clone(&metrics),
            );
            (handle, metrics)
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pending_queue_fifo_and_drain() {
        let env = TestEnv::new();
        let (handle, _) = env.spawn_handle();
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

    #[tokio::test(flavor = "multi_thread")]
    async fn publish_then_crash_recovery() {
        let env = TestEnv::new();
        let payload = vec![0xDEu8; 1500];
        {
            let (handle, _) = env.spawn_handle();
            handle.publish_payload(payload.clone()).await.unwrap();
            drop(handle);
        }
        let (handle2, _) = env.spawn_handle();
        let drained = handle2.drain_pending();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].size_bytes, payload.len() as u64);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn enqueue_idempotent() {
        let env = TestEnv::new();
        let payload = vec![0xEEu8; 1500];
        let (handle, _) = env.spawn_handle();
        let blob_id = handle.publish_payload(payload).await.unwrap();
        let blob_ref = BlobRef {
            blob_id,
            commitment: handle.blob_ref_commitment(&vec![0xEEu8; 1500]),
            size_bytes: 1500,
        };
        handle.test_enqueue(blob_ref, false);
        handle.test_enqueue(blob_ref, false);
        assert_eq!(handle.drain_pending().len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn drain_skips_already_attached() {
        let env = TestEnv::new();
        let (handle, _) = env.spawn_handle();
        let payload = vec![0xFFu8; 1500];
        let blob_id = handle.publish_payload(payload).await.unwrap();
        handle.mark_attached(blob_id).unwrap();
        assert!(handle.drain_pending().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn mark_attached_idempotent() {
        let env = TestEnv::new();
        let (handle, _) = env.spawn_handle();
        let blob_id = handle
            .publish_payload(vec![0xABu8; 1500])
            .await
            .unwrap();
        handle.mark_attached(blob_id).unwrap();
        handle.mark_attached(blob_id).unwrap();
        assert!(env.store.is_attached(&blob_id).unwrap());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ready_without_chunks_boot_skip() {
        let env = TestEnv::new();
        let blob_id = BlobId([0x99; 32]);
        let record = PublishRecord {
            state: PublishState::Ready as u8,
            blob_ref: BlobRef {
                blob_id,
                commitment: Hash32([0x01; 32]),
                size_bytes: 1024,
            },
        };
        let mut batch = storage::new_batch();
        blob_publish_store::put_ready_batch(&mut batch, &env.db, &blob_id, &record).unwrap();
        storage::wal::apply(&env.db, batch).unwrap();

        let (handle, _) = env.spawn_handle();
        assert!(handle.drain_pending().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn boot_sync_done_gate() {
        let env = TestEnv::new();
        let (handle, _) = env.spawn_handle();
        assert!(handle.boot_sync_done());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ledger_rehydrate_lightweight() {
        let env = TestEnv::new();
        let payload = vec![0x11u8; 1500];
        let blob_id = blob_id_from_payload(&payload);
        let size_bytes = payload.len() as u64;
        let shards = dag::erasure::encode_shards(
            &payload,
            &ErasureConfig {
                k: 4,
                n: 8,
                data_shard_size: 1024,
            },
        )
        .unwrap();
        let chunks = erasure_chunks(blob_id, size_bytes, &shards);
        let record = PublishRecord {
            state: PublishState::Ready as u8,
            blob_ref: BlobRef {
                blob_id,
                commitment: Hash32([0x22; 32]),
                size_bytes,
            },
        };
        env.store.publish_blob_atomic(&chunks, record).unwrap();

        let (handle, _) = env.spawn_handle();
        assert!(handle.is_available(&blob_id));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn confirm_fail_then_retry() {
        let env = TestEnv::new();
        let (handle, metrics) = env.spawn_handle();
        let missing = BlobId([0x77; 32]);
        assert!(handle.mark_attached(missing).is_err());
        assert_eq!(metrics.blob_mark_attached_fail_total.get(), 1);

        let blob_id = handle
            .publish_payload(vec![0x55u8; 1500])
            .await
            .unwrap();
        handle.mark_attached(blob_id).unwrap();
        assert!(env.store.is_attached(&blob_id).unwrap());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn confirm_attached_no_rebuild() {
        let env = TestEnv::new();
        let (handle, _) = env.spawn_handle();
        let blob_id = handle
            .publish_payload(vec![0x66u8; 1500])
            .await
            .unwrap();
        assert_eq!(handle.drain_pending().len(), 1);
        handle.mark_attached(blob_id).unwrap();
        drop(handle);

        let (handle2, _) = env.spawn_handle();
        assert!(handle2.drain_pending().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn boot_many_ready() {
        let env = TestEnv::new();
        for i in 0..50u8 {
            let payload = vec![i; 1500];
            let blob_id = blob_id_from_payload(&payload);
            let size_bytes = payload.len() as u64;
            let shards = dag::erasure::encode_shards(
                &payload,
                &ErasureConfig {
                    k: 4,
                    n: 8,
                    data_shard_size: 1024,
                },
            )
            .unwrap();
            let chunks = erasure_chunks(blob_id, size_bytes, &shards);
            let record = PublishRecord {
                state: PublishState::Ready as u8,
                blob_ref: BlobRef {
                    blob_id,
                    commitment: Hash32([i; 32]),
                    size_bytes,
                },
            };
            env.store.publish_blob_atomic(&chunks, record).unwrap();
        }
        let (handle, _) = env.spawn_handle();
        assert_eq!(handle.drain_pending().len(), 50);
    }
}
