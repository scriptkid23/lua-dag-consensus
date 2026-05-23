//! L3 local action applier smoke tests (plan 06b-l3).

use std::sync::Arc;

use borsh::to_vec;
use consensus::action::Action;
use node::{
    action_applier::ActionApplier,
    devnet_keys::devnet_valset_four,
    host_context::{ChainedBeacon, StubHostBundle},
    live_dag::LiveDag,
    observability::metrics::Metrics,
    query::RocksConsensusQuery,
    timer::TimerRegistry,
};
use storage::{Database, RocksPersistence};
use tokio::sync::mpsc;
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    macros::{AggregationMode, MacroCheckpoint, MacroQc},
    primitives::{Epoch, Height},
};

fn test_applier() -> (ActionApplier, RocksPersistence) {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(&storage::StorageConfig {
        path: dir.path().to_path_buf(),
        create_if_missing: true,
        max_total_wal_size_mb: 16,
    })
    .unwrap());
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
fn applier_persists_macro_qc_and_checkpoint() {
    let (mut applier, persistence) = test_applier();
    let cp = MacroCheckpoint {
        height: Height(3),
        epoch: Epoch(0),
        parent: Hash32::zero(),
        micro_root: Hash32([1; 32]),
        hash: Hash32([2; 32]),
    };
    let qc = MacroQc {
        checkpoint_hash: cp.hash,
        mode: AggregationMode::Mode0Flat,
        agg: BlsAggSig {
            sig: BlsSig([0; 96]),
            bitmap: vec![0xFF],
        },
    };
    applier
        .apply(&Action::PersistMacroCheckpoint(cp.clone()))
        .unwrap();
    applier.apply(&Action::PersistMacroQc(qc.clone())).unwrap();
    assert_eq!(
        persistence
            .macro_checkpoint_at(cp.height)
            .unwrap()
            .unwrap()
            .hash,
        cp.hash
    );
    assert_eq!(persistence.macro_qc_for(&cp.hash).unwrap().unwrap(), qc);
}

#[test]
fn host_bundle_signer_matches_valset() {
    let valset = devnet_valset_four();
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(
        Database::open(&storage::StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 16,
        })
        .unwrap(),
    );
    let dag = Arc::new(LiveDag::new(db));
    let bundle = StubHostBundle::new("node1", valset, dag, None).unwrap();
    assert!(
        bundle
            .signer
            .sign_bls(b"dst", b"msg")
            .0
            .iter()
            .any(|&b| b != 0)
    );
}

#[test]
fn query_latest_finalized_after_persist() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(&storage::StorageConfig {
        path: dir.path().to_path_buf(),
        create_if_missing: true,
        max_total_wal_size_mb: 16,
    })
    .unwrap());
    let persistence = RocksPersistence::new(db);
    let cp = MacroCheckpoint {
        height: Height(5),
        epoch: Epoch(0),
        parent: Hash32::zero(),
        micro_root: Hash32([4; 32]),
        hash: Hash32([5; 32]),
    };
    let qc = MacroQc {
        checkpoint_hash: cp.hash,
        mode: AggregationMode::Mode0Flat,
        agg: BlsAggSig {
            sig: BlsSig([0; 96]),
            bitmap: vec![0xFF],
        },
    };
    persistence.store_macro_checkpoint(&cp).unwrap();
    persistence.store_macro_qc(&qc).unwrap();

    let query = RocksConsensusQuery::new(persistence);
    let got = query.latest_finalized().unwrap().unwrap();
    assert_eq!(got.checkpoint_hash, cp.hash);
}

#[test]
fn macro_checkpoint_query_roundtrip_encoding() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(&storage::StorageConfig {
        path: dir.path().to_path_buf(),
        create_if_missing: true,
        max_total_wal_size_mb: 16,
    })
    .unwrap());
    let persistence = RocksPersistence::new(db);
    let cp = MacroCheckpoint {
        height: Height(9),
        epoch: Epoch(0),
        parent: Hash32::zero(),
        micro_root: Hash32([6; 32]),
        hash: Hash32([7; 32]),
    };
    persistence.store_macro_checkpoint(&cp).unwrap();
    let query = RocksConsensusQuery::new(persistence);
    let got = query.macro_checkpoint_at(Height(9)).unwrap().unwrap();
    assert_eq!(to_vec(&got).unwrap(), to_vec(&cp).unwrap());
}
