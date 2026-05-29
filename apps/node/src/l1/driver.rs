//! Micro-round tick loop that produces certified vertices (plan 06b-L1).

use std::sync::Arc;
use std::time::Duration;

use consensus::{config::Config, event::Event};
use net::gossip::Topic;
use tokio::sync::mpsc;
use tracing::warn;
use types::{
    dag::BlobRef,
    validator::ValidatorSet,
};

use crate::{
    blob::BlobCustodyHandle,
    host_context::ChainedBeacon,
    live_dag::LiveDag,
    l1::{
        parent::parent_hash_for_round,
        vertex_builder::{build_quorum_vertices_with_blobs, quorum_vertex_count},
    },
    observability::metrics::Metrics,
};

/// Host-side L1 feed: builds quorum vertices each micro-round.
#[derive(Debug)]
pub struct L1Driver {
    virtual_round: u64,
    valset: ValidatorSet,
    config: Config,
    dag: Arc<LiveDag>,
    beacon: Arc<ChainedBeacon>,
    events_tx: mpsc::Sender<Event>,
    publish_tx: mpsc::Sender<(Topic, Vec<u8>)>,
    round_duration: Duration,
    real_vertex_certs: bool,
    blob_custody: Option<BlobCustodyHandle>,
    metrics: Arc<Metrics>,
}

impl L1Driver {
    /// Build a driver wired to the orchestrator event loop and gossip publish channel.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        valset: ValidatorSet,
        config: Config,
        dag: Arc<LiveDag>,
        beacon: Arc<ChainedBeacon>,
        events_tx: mpsc::Sender<Event>,
        publish_tx: mpsc::Sender<(Topic, Vec<u8>)>,
        round_duration: Duration,
        real_vertex_certs: bool,
        blob_custody: Option<BlobCustodyHandle>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            virtual_round: 0,
            valset,
            config,
            dag,
            beacon,
            events_tx,
            publish_tx,
            round_duration,
            real_vertex_certs,
            blob_custody,
            metrics,
        }
    }

    /// Run until the publish or events channel closes.
    pub async fn run(mut self) {
        let mut interval = tokio::time::interval(self.round_duration);
        loop {
            interval.tick().await;
            if !self.tick_round().await {
                break;
            }
        }
    }

    /// One micro-round tick. Returns `false` if the events channel closed.
    async fn tick_round(&mut self) -> bool {
        let parent = match parent_hash_for_round(
            self.virtual_round,
            &*self.dag,
            &self.config,
            &self.valset,
            self.beacon.as_ref(),
        ) {
            Ok(p) => p,
            Err(e) => {
                warn!(target: "node::l1_driver", error = %e, "parent hash lookup failed");
                None
            }
        };

        let pending_blobs = self.pending_blobs_for_round();
        let batch = build_quorum_vertices_with_blobs(
            self.virtual_round,
            &self.valset,
            parent,
            self.real_vertex_certs,
            pending_blobs,
        );
        for cv in batch {
            if self.real_vertex_certs {
                if let Err(e) = dag::cert::verify_certified_vertex(&cv, &self.valset) {
                    warn!(target: "node::l1_driver", error = %e, "rejecting locally built vertex");
                    continue;
                }
            }
            if let Err(e) = self.dag.ingest(cv.clone()) {
                warn!(target: "node::l1_driver", error = %e, "ingest failed");
                continue;
            }
            match net::gossip_wire::encode_certified_vertex(&cv) {
                Ok((topic, bytes)) => {
                    if let Err(e) = self.publish_tx.try_send((topic, bytes)) {
                        warn!(target: "node::l1_driver", error = %e, "publish channel full/closed");
                    }
                }
                Err(e) => {
                    warn!(target: "node::l1_driver", error = %e, "encode certified vertex failed");
                }
            }
            if self
                .events_tx
                .send(Event::CertifiedVertexReceived(cv))
                .await
                .is_err()
            {
                return false;
            }
        }
        self.virtual_round += 1;
        true
    }

    fn pending_blobs_for_round(&self) -> Vec<BlobRef> {
        self.blob_custody
            .as_ref()
            .map(|c| c.drain_pending())
            .unwrap_or_default()
    }

    /// Quorum size for the configured validator set (test helper).
    #[must_use]
    pub fn quorum_size(&self) -> u32 {
        quorum_vertex_count(u32::try_from(self.valset.entries.len()).unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devnet_keys::devnet_valset_four;
    use consensus::event::Event;
    use storage::{config::StorageConfig, db::Database};

    #[tokio::test]
    async fn tick_emits_quorum_events_per_round() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(
            Database::open(&StorageConfig {
                path: dir.path().to_path_buf(),
                create_if_missing: true,
                max_total_wal_size_mb: 16,
            })
            .unwrap(),
        );
        let dag = Arc::new(LiveDag::new(db));
        let valset = devnet_valset_four();
        let config = consensus::Config::default_table_17_1();
        let beacon = Arc::new(ChainedBeacon::new());
        let (events_tx, mut events_rx) = mpsc::channel(64);
        let (publish_tx, mut publish_rx) = mpsc::channel(64);
        let metrics = Arc::new(Metrics::new().unwrap());

        let mut driver = L1Driver::new(
            valset,
            config,
            dag,
            beacon,
            events_tx,
            publish_tx,
            Duration::from_millis(10_000),
            false,
            None,
            metrics,
        );
        let quorum = driver.quorum_size();

        tokio::spawn(async move {
            while publish_rx.recv().await.is_some() {}
        });

        assert!(driver.tick_round().await);
        let mut received = 0usize;
        while received < quorum as usize {
            let ev = events_rx.recv().await.expect("event");
            assert!(matches!(ev, Event::CertifiedVertexReceived(_)));
            received += 1;
        }
        assert_eq!(received, quorum as usize);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pending_blobs_attached_round_robin() {
        use crate::blob::{BlobCustody, BlobCustodyConfig, RocksBlobStore};
        use dag::blob::store::BlobStore as BlobStoreTrait;

        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(
            Database::open(&StorageConfig {
                path: dir.path().to_path_buf(),
                create_if_missing: true,
                max_total_wal_size_mb: 16,
            })
            .unwrap(),
        );
        let dag = Arc::new(LiveDag::new(Arc::clone(&db)));
        let valset = devnet_valset_four();
        let config = consensus::Config::default_table_17_1();
        let beacon = Arc::new(ChainedBeacon::new());
        let (events_tx, mut events_rx) = mpsc::channel(64);
        let (publish_tx, mut publish_rx) = mpsc::channel(256);
        tokio::spawn(async move {
            while publish_rx.recv().await.is_some() {}
        });
        let metrics = Arc::new(Metrics::new().unwrap());

        let store: Arc<dyn BlobStoreTrait> = Arc::new(RocksBlobStore::new(Arc::clone(&db)));
        let (_chunks_tx, chunks_rx) = mpsc::channel(64);
        let custody = BlobCustody::spawn(
            store,
            chunks_rx,
            publish_tx.clone(),
            BlobCustodyConfig {
                chunk_size: 1024,
                erasure: None,
            },
            metrics.clone(),
        );

        let mut submitted_ids = Vec::new();
        for i in 0u8..5 {
            let payload = vec![0xA0u8 ^ i; 1500];
            submitted_ids.push(custody.publish_payload(payload).await.unwrap());
        }

        let mut driver = L1Driver::new(
            valset.clone(),
            config,
            dag,
            beacon,
            events_tx,
            publish_tx,
            Duration::from_millis(10_000),
            true,
            Some(custody.clone()),
            metrics,
        );
        let quorum = driver.quorum_size();
        assert_eq!(quorum, 3);

        assert!(driver.tick_round().await);

        let mut received: Vec<types::dag::CertifiedVertex> = Vec::new();
        for _ in 0..quorum {
            let ev = events_rx.recv().await.expect("event");
            let Event::CertifiedVertexReceived(cv) = ev else {
                panic!("expected CertifiedVertexReceived");
            };
            received.push(cv);
        }

        // Per-slot counts: 5 blobs, quorum=3 → [2,2,1].
        let mut counts: Vec<usize> = received.iter().map(|cv| cv.vertex.blobs.len()).collect();
        counts.sort_unstable();
        assert_eq!(counts, vec![1, 2, 2]);

        let total: usize = received.iter().map(|cv| cv.vertex.blobs.len()).sum();
        assert_eq!(total, 5);

        let mut seen_ids: Vec<_> = received
            .iter()
            .flat_map(|cv| cv.vertex.blobs.iter().map(|b| b.blob_id))
            .collect();
        seen_ids.sort();
        let mut expected_ids = submitted_ids.clone();
        expected_ids.sort();
        assert_eq!(seen_ids, expected_ids);

        for cv in &received {
            dag::cert::verify_certified_vertex(cv, &valset).expect("real cert verifies");
        }

        assert!(custody.drain_pending().is_empty());
    }
}
