//! Micro-round tick loop that produces certified vertices (plan 06b-L1).

use std::sync::Arc;
use std::time::Duration;

use consensus::{config::Config, event::Event};
use net::gossip::Topic;
use tokio::sync::mpsc;
use tracing::warn;
use types::validator::ValidatorSet;

use crate::{
    host_context::ChainedBeacon,
    live_dag::LiveDag,
    l1::{
        parent::parent_hash_for_round,
        vertex_builder::{build_quorum_vertices_for_valset, quorum_vertex_count},
    },
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
}

impl L1Driver {
    /// Build a driver wired to the orchestrator event loop and gossip publish channel.
    #[must_use]
    pub fn new(
        valset: ValidatorSet,
        config: Config,
        dag: Arc<LiveDag>,
        beacon: Arc<ChainedBeacon>,
        events_tx: mpsc::Sender<Event>,
        publish_tx: mpsc::Sender<(Topic, Vec<u8>)>,
        round_duration: Duration,
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

        let batch = build_quorum_vertices_for_valset(self.virtual_round, &self.valset, parent);
        for cv in batch {
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

        let mut driver = L1Driver::new(
            valset,
            config,
            dag,
            beacon,
            events_tx,
            publish_tx,
            Duration::from_millis(10_000),
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
