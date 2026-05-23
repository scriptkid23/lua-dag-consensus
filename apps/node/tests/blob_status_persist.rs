//! ActionApplier persists `UpdateBlobStatus` monotonically (plan BlobStatus persist).

use std::sync::Arc;

use consensus::{action::Action, api::tier::BlobStatus};
use node::{
    action_applier::ActionApplier,
    host_context::ChainedBeacon,
    observability::metrics::Metrics,
    timer::TimerRegistry,
};
use storage::{Database, RocksPersistence};
use tokio::sync::mpsc;
use types::primitives::BlobId;

fn test_applier() -> (ActionApplier, RocksPersistence) {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(
        Database::open(&storage::StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 16,
        })
        .unwrap(),
    );
    let persistence = RocksPersistence::new(Arc::clone(&db));
    let (timer_tx, _timer_rx) = mpsc::channel(8);
    let registry = Arc::new(TimerRegistry::default());
    let beacon = Arc::new(ChainedBeacon::new());
    let metrics = Arc::new(Metrics::new().unwrap());
    let applier = ActionApplier::new(
        persistence.clone(),
        timer_tx,
        registry,
        beacon,
        metrics,
    );
    (applier, persistence)
}

#[test]
fn applier_persists_monotonic_status() {
    let (mut applier, persistence) = test_applier();
    let blob = BlobId([1; 32]);
    applier
        .apply(&Action::UpdateBlobStatus {
            blob,
            status: BlobStatus::SoftConfirmed,
        })
        .unwrap();
    applier
        .apply(&Action::UpdateBlobStatus {
            blob,
            status: BlobStatus::Justified,
        })
        .unwrap();
    applier
        .apply(&Action::UpdateBlobStatus {
            blob,
            status: BlobStatus::SoftConfirmed,
        })
        .unwrap();
    assert_eq!(
        persistence.blob_status(&blob).unwrap(),
        BlobStatus::Justified
    );
}

#[test]
fn unknown_blob_defaults_to_accepted() {
    let (_applier, persistence) = test_applier();
    assert_eq!(
        persistence.blob_status(&BlobId([0xAB; 32])).unwrap(),
        BlobStatus::Accepted
    );
}
