//! Micro-round tick loop that produces certified vertices (plan 06b-L1).

use std::sync::Arc;
use std::time::Duration;

use consensus::{config::Config, event::Event};
use crypto::hash::{blake3_with_dst, dst};
use dag::blob::commit::{blob_commitment, blob_id_from_payload};
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
    demo_blob_enabled: bool,
    demo_blob_every_n_rounds: u64,
    chunk_size: u32,
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
        demo_blob_enabled: bool,
        demo_blob_every_n_rounds: u64,
        chunk_size: u32,
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
            demo_blob_enabled,
            demo_blob_every_n_rounds,
            chunk_size,
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

        let demo_blobs = self.demo_blobs_for_round().await;
        let batch = build_quorum_vertices_with_blobs(
            self.virtual_round,
            &self.valset,
            parent,
            self.real_vertex_certs,
            demo_blobs,
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

    async fn demo_blobs_for_round(&mut self) -> Vec<BlobRef> {
        if !self.demo_blob_enabled {
            return vec![];
        }
        let Some(custody) = &self.blob_custody else {
            return vec![];
        };
        if self.demo_blob_every_n_rounds == 0
            || self.virtual_round % self.demo_blob_every_n_rounds != 0
        {
            return vec![];
        }

        let payload = demo_blob_payload(self.virtual_round, self.chunk_size);
        let blob_id = blob_id_from_payload(&payload);
        if !custody.is_available(&blob_id) {
            if let Err(e) = custody.publish_payload(payload.clone()).await {
                warn!(target: "node::l1_driver", error = %e, "demo blob publish failed");
            }
        }
        if !custody.is_available(&blob_id) {
            warn!(
                target: "node::l1_driver",
                ?blob_id,
                "demo blob not locally available; skipping BlobRef attachment"
            );
            self.metrics.blob_custody_missing.inc();
            return vec![];
        }

        vec![BlobRef {
            blob_id,
            commitment: blob_commitment(&payload),
            size_bytes: u64::try_from(payload.len()).expect("payload fits u64"),
        }]
    }

    /// Quorum size for the configured validator set (test helper).
    #[must_use]
    pub fn quorum_size(&self) -> u32 {
        quorum_vertex_count(u32::try_from(self.valset.entries.len()).unwrap_or(0))
    }
}

/// Deterministic multi-chunk demo payload keyed by virtual round.
#[must_use]
pub fn demo_blob_payload(round: u64, chunk_size: u32) -> Vec<u8> {
    let seed = blake3_with_dst(dst::SIM_VERTEX_HASH, &round.to_be_bytes());
    let len = usize::from(chunk_size) + 1024;
    let mut payload = vec![0u8; len];
    payload[..32].copy_from_slice(seed.as_bytes());
    payload
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
            false,
            8,
            65_536,
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
}
