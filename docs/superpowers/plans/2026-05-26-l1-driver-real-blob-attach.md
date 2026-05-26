# L1Driver Real Blob Attach — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the L1Driver synthetic demo-blob scheduler and connect `lua_submitBlob` RPC submissions to vertex headers via a pending-attach queue inside `BlobCustodyHandle`, drained per micro-round and round-robin–distributed across the 2f+1 quorum authors.

**Architecture:** `BlobCustodyHandle` gains a `pending: Arc<Mutex<VecDeque<BlobRef>>>` queue, populated as the last step of `publish_payload` (after chunks are stored + gossiped) and drained each tick by `L1Driver::tick_round`. `build_quorum_vertices_with_blobs` switches from cloning the same blob list into every vertex to partitioning blobs by `j % quorum` so each blob appears in exactly one author's vertex header. Demo path (config keys, fields, fn, devnet.toml lines, unused metric) is deleted.

**Tech Stack:** Rust 2021, tokio, axum, libp2p gossipsub (already wired), `serde`/toml profile loader, `prometheus` IntCounter, BLAKE3 via `crypto::hash`.

---

## File Structure

**Modified (no new files, no new modules):**

| File | Responsibility after change |
|---|---|
| `apps/node/src/blob/mod.rs` | Owns chunk store + ingest loop + publish path + **NEW pending-attach queue**. Methods added: `drain_pending` (pub), `enqueue_pending` (priv). `publish_payload` calls `enqueue_pending` after the chunk loop. |
| `apps/node/src/l1/vertex_builder.rs` | Partitions `Vec<BlobRef>` by `j % quorum` across 2f+1 vertex slots instead of cloning. Signature unchanged. |
| `apps/node/src/l1/driver.rs` | `L1Driver` loses three demo fields (`demo_blob_enabled`, `demo_blob_every_n_rounds`, `chunk_size`). `demo_blobs_for_round` and free fn `demo_blob_payload` deleted. New helper `pending_blobs_for_round` calls `custody.drain_pending()`. |
| `apps/node/src/runtime.rs` | `L1Driver::new` call site loses 3 demo args. |
| `apps/node/src/config_layers.rs` | `NodeSection` loses `l1_demo_blob_enabled`, `demo_blob_every_n_rounds`, and `default_demo_blob_every` helper. |
| `apps/node/src/observability/metrics.rs` | Drops `blob_custody_missing` IntCounter (was only incremented by the demo path). |
| `apps/node/tests/l1_driver_smoke.rs` | `L1Driver::new` call site updated to new shape. |
| `config/profiles/devnet.toml` | Removes `l1_demo_blob_enabled = true` and `demo_blob_every_n_rounds = 8`. |

**No other files touched.** `crates/dag/*`, wire formats, RPC schema, and `types/dag/refs.rs::BlobRef` are unchanged.

---

## Task 1: Pending queue inside `BlobCustodyHandle`

**Files:**
- Modify: `apps/node/src/blob/mod.rs`

- [ ] **Step 1: Write the failing test**

Append to the existing `apps/node/src/blob/mod.rs` (no `#[cfg(test)] mod tests` exists yet — add one at the bottom of the file):

```rust
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
                chunk_size: 1024,
                erasure: None,
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p node --lib blob::tests::pending_queue_fifo_and_drain --locked`
Expected: compile error — `method drain_pending not found on BlobCustodyHandle`.

- [ ] **Step 3: Add the queue field and methods**

Edit `apps/node/src/blob/mod.rs`:

1. Add `VecDeque` to the std import:

```rust
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
```

2. Add `BlobRef` to the `types` import (next to `ChunkRef`):

```rust
use types::{crypto_types::Hash32, dag::{BlobRef, ChunkRef}, primitives::BlobId};
```

3. Add the `pending` field to `BlobCustodyHandle`:

```rust
#[derive(Clone)]
pub struct BlobCustodyHandle {
    store: Arc<dyn BlobStore>,
    ledger: Arc<Mutex<CustodyLedger>>,
    publish_tx: mpsc::Sender<(Topic, Vec<u8>)>,
    config: BlobCustodyConfig,
    metrics: Arc<Metrics>,
    pending: Arc<Mutex<VecDeque<BlobRef>>>,
}
```

4. Update the doc comment on the type:

```rust
/// Shared handle for RPC publish + L1 driver pending-attach drain.
```

5. Initialize the field in `BlobCustody::spawn` (look for the struct literal that builds `handle` around current line 137):

```rust
        let handle = BlobCustodyHandle {
            store: Arc::clone(&store),
            ledger: Arc::clone(&ledger),
            publish_tx,
            config: config.clone(),
            metrics: Arc::clone(&metrics),
            pending: Arc::new(Mutex::new(VecDeque::new())),
        };
```

6. Add the two new methods inside the existing `impl BlobCustodyHandle` block (place them after `list_chunk_refs`, before `publish_payload`):

```rust
    /// Pop every queued `BlobRef` in FIFO order. Called by `L1Driver` each tick.
    #[must_use]
    pub fn drain_pending(&self) -> Vec<BlobRef> {
        let mut q = self.pending.lock().expect("lock");
        q.drain(..).collect()
    }

    fn enqueue_pending(&self, blob: BlobRef) {
        self.pending.lock().expect("lock").push_back(blob);
    }
```

7. Append the enqueue call at the end of `publish_payload` (just before `Ok(blob_id)`):

```rust
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

        self.enqueue_pending(BlobRef {
            blob_id,
            commitment: self.blob_ref_commitment(&payload),
            size_bytes,
        });
        Ok(blob_id)
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p node --lib blob::tests::pending_queue_fifo_and_drain --locked`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/node/src/blob/mod.rs
git commit -m "feat(node): pending-attach queue on BlobCustodyHandle"
```

---

## Task 2: Partition blobs across quorum slots in `vertex_builder`

**Files:**
- Modify: `apps/node/src/l1/vertex_builder.rs`

- [ ] **Step 1: Write the failing test**

Inside `apps/node/src/l1/vertex_builder.rs`'s existing `#[cfg(test)] mod tests` block (around line 139), add:

```rust
    #[test]
    fn blobs_partition_round_robin_across_quorum_slots() {
        use types::crypto_types::Hash32;
        let valset = devnet_valset_four();
        let mk = |tag: u8| BlobRef {
            blob_id: types::primitives::BlobId([tag; 32]),
            commitment: Hash32([tag; 32]),
            size_bytes: u64::from(tag) * 100,
        };
        let blobs = vec![mk(1), mk(2), mk(3), mk(4), mk(5)];
        let batch = build_quorum_vertices_with_blobs(7, &valset, None, false, blobs);
        assert_eq!(batch.len(), 3);
        // j % 3: 0,1,2,0,1 → slot0=[1,4], slot1=[2,5], slot2=[3].
        assert_eq!(batch[0].vertex.blobs.len(), 2);
        assert_eq!(batch[0].vertex.blobs[0].blob_id.0[0], 1);
        assert_eq!(batch[0].vertex.blobs[1].blob_id.0[0], 4);
        assert_eq!(batch[1].vertex.blobs.len(), 2);
        assert_eq!(batch[1].vertex.blobs[0].blob_id.0[0], 2);
        assert_eq!(batch[1].vertex.blobs[1].blob_id.0[0], 5);
        assert_eq!(batch[2].vertex.blobs.len(), 1);
        assert_eq!(batch[2].vertex.blobs[0].blob_id.0[0], 3);
    }

    #[test]
    fn empty_blob_list_yields_empty_buckets_for_all_authors() {
        let valset = devnet_valset_four();
        let batch = build_quorum_vertices_with_blobs(0, &valset, None, false, vec![]);
        assert_eq!(batch.len(), 3);
        assert!(batch.iter().all(|cv| cv.vertex.blobs.is_empty()));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p node --lib l1::vertex_builder::tests::blobs_partition_round_robin_across_quorum_slots --locked`
Expected: FAIL — current `build_quorum_vertices_with_blobs` clones the full list into every vertex, so `batch[0].vertex.blobs.len()` will be 5, not 2.

- [ ] **Step 3: Implement the partition**

Replace `build_quorum_vertices_with_blobs` (around current line 114) with:

```rust
/// Build `2f+1` certified vertices, partitioning `blobs` round-robin across
/// quorum slots (slot `i` receives blob `j` where `j % quorum == i`).
#[must_use]
pub fn build_quorum_vertices_with_blobs(
    round: u64,
    valset: &ValidatorSet,
    parent_hash: Option<Hash32>,
    real_certs: bool,
    blobs: Vec<BlobRef>,
) -> Vec<CertifiedVertex> {
    let n = u32::try_from(valset.entries.len()).expect("validator count fits u32");
    let quorum = quorum_vertex_count(n);
    let quorum_usize = quorum as usize;

    let mut buckets: Vec<Vec<BlobRef>> = (0..quorum_usize).map(|_| Vec::new()).collect();
    for (j, b) in blobs.into_iter().enumerate() {
        buckets[j % quorum_usize].push(b);
    }

    (0..quorum)
        .map(|i| {
            let idx = usize::try_from((round + u64::from(i)) % u64::from(n)).expect("index");
            let author = valset.entries[idx].id;
            build_certified_vertex_with_blobs(
                round,
                author,
                parent_hash,
                real_certs,
                valset,
                std::mem::take(&mut buckets[i as usize]),
            )
        })
        .collect()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run all vertex_builder tests (the existing `builds_quorum_for_devnet_four`, `real_certs_verify_against_devnet_valset`, `sibling_vertices_in_same_round_have_distinct_hashes` must still pass — they call through `build_quorum_vertices_for_valset` with `vec![]`):

```
cargo test -p node --lib l1::vertex_builder::tests --locked
```

Expected: PASS for all 5 tests (3 existing + 2 new).

- [ ] **Step 5: Commit**

```bash
git add apps/node/src/l1/vertex_builder.rs
git commit -m "feat(node): partition vertex blobs round-robin across quorum slots"
```

---

## Task 3: Remove demo from `L1Driver` and add round-robin attach test

**Files:**
- Modify: `apps/node/src/l1/driver.rs`

- [ ] **Step 1: Update the existing unit test signature**

In `apps/node/src/l1/driver.rs`'s `#[cfg(test)] mod tests` block (around line 207), replace the `L1Driver::new` call inside `tick_emits_quorum_events_per_round` (currently 13 args, ending in `false, 8, 65_536, metrics`) with the new 10-arg shape:

```rust
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
```

- [ ] **Step 2: Add the new failing test**

Append a second test inside the same `mod tests` block (after `tick_emits_quorum_events_per_round`):

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn pending_blobs_attached_round_robin() {
        use crate::blob::{BlobCustody, BlobCustodyConfig, RocksBlobStore};
        use crate::observability::metrics::Metrics;
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
            match events_rx.recv().await.expect("event") {
                consensus::event::Event::CertifiedVertexReceived(cv) => received.push(cv),
            }
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
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p node --lib l1::driver::tests --locked`
Expected: compile error or first test passes / second test fails — the driver still has demo fields and signature mismatch.

- [ ] **Step 4: Refactor `L1Driver` source**

In `apps/node/src/l1/driver.rs`:

1. Replace the import block at the top of the file (currently lines 1-26):

```rust
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
```

(Removed: `crypto::hash::{blake3_with_dst, dst}`, `dag::blob::commit::blob_id_from_payload`.)

2. Replace the struct definition:

```rust
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
```

3. Replace `L1Driver::new`:

```rust
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
```

4. In `tick_round`, replace the `demo_blobs` call:

```rust
        let pending_blobs = self.pending_blobs_for_round();
        let batch = build_quorum_vertices_with_blobs(
            self.virtual_round,
            &self.valset,
            parent,
            self.real_vertex_certs,
            pending_blobs,
        );
```

5. Replace `demo_blobs_for_round` (entire async fn and its body) and the standalone `demo_blob_payload` function (lines ~197-205) with one synchronous helper:

```rust
    fn pending_blobs_for_round(&self) -> Vec<BlobRef> {
        self.blob_custody
            .as_ref()
            .map(|c| c.drain_pending())
            .unwrap_or_default()
    }
```

Delete the previous `async fn demo_blobs_for_round` and the `pub fn demo_blob_payload` definitions entirely.

6. Remove unused field reference — the `metrics: Arc<Metrics>` field stays (still used elsewhere). Verify no remaining references to `self.demo_blob_enabled`, `self.demo_blob_every_n_rounds`, `self.chunk_size`. They should all be gone after step 2.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p node --lib l1::driver::tests --locked`
Expected: PASS for both `tick_emits_quorum_events_per_round` and `pending_blobs_attached_round_robin`.

- [ ] **Step 6: Commit**

```bash
git add apps/node/src/l1/driver.rs
git commit -m "feat(node): drain BlobCustodyHandle pending queue into vertex headers (drop demo path)"
```

---

## Task 4: Update `runtime.rs` call site

**Files:**
- Modify: `apps/node/src/runtime.rs`

- [ ] **Step 1: Update the `L1Driver::new` call**

In `apps/node/src/runtime.rs` (around line 229), replace the existing 13-arg `L1Driver::new(...)` call with:

```rust
        let driver = L1Driver::new(
            valset.clone(),
            cfg.consensus.clone(),
            Arc::clone(&live_dag),
            Arc::clone(&host_bundle.beacon),
            events_tx.clone(),
            publish_tx,
            std::time::Duration::from_millis(round_ms),
            cfg.node.l1_real_vertex_certs,
            if cfg.node.l1_blob_custody_enabled {
                blob_custody_handle.clone()
            } else {
                None
            },
            metrics.clone(),
        );
```

- [ ] **Step 2: Verify the package builds**

Run: `cargo check -p node --locked`
Expected: PASS (`cfg.node.l1_demo_blob_enabled` and `cfg.node.demo_blob_every_n_rounds` and `cfg.node.blob_chunk_size_bytes` were removed from the call — `blob_chunk_size_bytes` is still referenced elsewhere in runtime.rs for `BlobCustodyConfig::chunk_size`, that stays).

If a compile error mentions a removed field, scroll up in runtime.rs and confirm only the L1Driver::new arglist was touched.

- [ ] **Step 3: Commit**

```bash
git add apps/node/src/runtime.rs
git commit -m "refactor(node): runtime L1Driver::new no longer threads demo args"
```

---

## Task 5: Drop demo fields from config

**Files:**
- Modify: `apps/node/src/config_layers.rs`
- Modify: `config/profiles/devnet.toml`

- [ ] **Step 1: Remove demo fields from `NodeSection`**

In `apps/node/src/config_layers.rs`, delete these three items:

- Field `pub l1_demo_blob_enabled: bool,` (currently lines 61-63 including the comment + `#[serde(default)]`).
- Field `pub demo_blob_every_n_rounds: u64,` (currently lines 64-66 including the comment + `#[serde(default = "default_demo_blob_every")]`).
- Free function `fn default_demo_blob_every() -> u64 { 8 }` (currently lines 97-99).

Leave `l1_blob_custody_enabled`, `blob_chunk_size_bytes`, `default_blob_chunk_size`, and all erasure fields untouched.

- [ ] **Step 2: Remove demo lines from devnet profile**

In `config/profiles/devnet.toml`, delete lines 32-33:

```
l1_demo_blob_enabled = true
demo_blob_every_n_rounds = 8
```

Leave the comment `# Blob payload custody + chunk gossip (plan 07b).` and the surviving keys `l1_blob_custody_enabled = true`, `blob_chunk_size_bytes = 65536`.

- [ ] **Step 3: Verify build**

Run: `cargo check -p node --locked`
Expected: PASS.

If `toml::from_str` parses `local.toml` strictly and rejects unknown keys, an operator with old `local.toml` will get a runtime error on startup. Acceptable per the spec; no migration code in this plan.

- [ ] **Step 4: Verify there are no remaining references to demo**

Run: `rg -n "l1_demo_blob_enabled|demo_blob_every|demo_blob_payload|demo_blobs_for_round|default_demo_blob_every" apps/ crates/ config/`
Expected: zero hits (matches in `docs/` are fine — historical plan docs remain as record).

- [ ] **Step 5: Commit**

```bash
git add apps/node/src/config_layers.rs config/profiles/devnet.toml
git commit -m "chore(config): remove l1_demo_blob_enabled and demo_blob_every_n_rounds"
```

---

## Task 6: Drop unused `blob_custody_missing` metric

**Files:**
- Modify: `apps/node/src/observability/metrics.rs`

- [ ] **Step 1: Check the metric is truly unreferenced after Task 3**

Run: `rg -n "blob_custody_missing" apps/ crates/`
Expected: hits only inside `apps/node/src/observability/metrics.rs` (definition, registration, struct field). If any other file still increments it, stop and re-check Task 3 — that file should have removed the only consumer.

- [ ] **Step 2: Remove the metric definition, registration, and struct field**

In `apps/node/src/observability/metrics.rs`:

1. Delete the struct field around line 31:

```rust
    pub blob_custody_missing: IntCounter,
```

(plus its doc comment if present)

2. Delete the constructor block around lines 84-87:

```rust
        let blob_custody_missing = IntCounter::new(
            "node_blob_custody_missing_total",
            "Vertex blob refs skipped because payload is not locally available",
        )?;
```

3. Delete the registry registration around line 92:

```rust
        registry.register(Box::new(blob_custody_missing.clone()))?;
```

(Line 91 — `registry.register(Box::new(blob_chunk_rejected.clone()))?;` — stays.)

4. Delete the struct-literal entry around line 105 inside the `Ok(Self { ... })` block:

```rust
            blob_custody_missing,
```

(Line 104 — `blob_chunk_rejected,` — stays.)

- [ ] **Step 3: Verify build**

Run: `cargo check -p node --locked`
Expected: PASS.

- [ ] **Step 4: Verify metric is gone**

Run: `rg -n "blob_custody_missing" apps/ crates/`
Expected: zero hits.

- [ ] **Step 5: Commit**

```bash
git add apps/node/src/observability/metrics.rs
git commit -m "chore(metrics): drop unused blob_custody_missing counter"
```

---

## Task 7: Fix `l1_driver_smoke.rs` integration test

**Files:**
- Modify: `apps/node/tests/l1_driver_smoke.rs`

**Background:** This test was already calling `L1Driver::new` with only 8 args at HEAD (a pre-existing build break unrelated to this plan). After Task 3, the constructor takes 10 args. Update the call to match the new shape.

- [ ] **Step 1: Update the call site**

In `apps/node/tests/l1_driver_smoke.rs` (around line 88), replace the 8-arg call with:

```rust
    let driver = L1Driver::new(
        valset,
        cfg,
        live_dag,
        beacon,
        events_tx,
        publish_tx,
        Duration::from_millis(50),
        true,
        None,
        metrics.clone(),
    );
```

`metrics` is already defined on line 61 of the test. `None` means no blob custody — this test doesn't exercise blob attach, so an empty pending queue is correct.

- [ ] **Step 2: Verify the test compiles**

Run: `cargo check -p node --tests --locked`
Expected: PASS for `l1_driver_smoke.rs`. (A separate pre-existing import error in `apps/node/tests/causal_set_rpc.rs:7` for `ValidatorId` may still fail — that is out of scope for this plan and must be left as-is.)

- [ ] **Step 3: Run the smoke test**

Run: `cargo test -p node --test l1_driver_smoke --locked`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/node/tests/l1_driver_smoke.rs
git commit -m "test(node): update l1_driver_smoke for new L1Driver::new shape"
```

---

## Task 8: End-to-end verification

**Files:** none modified — verification only.

- [ ] **Step 1: Run the full node test suite**

Run: `cargo test -p node --locked`
Expected: PASS for every test in `apps/node` that was passing at the start of this branch. The pre-existing `causal_set_rpc.rs` import bug remains out of scope; if it now blocks compilation of other tests, document and skip.

- [ ] **Step 2: Run the workspace check**

Run: `cargo check --workspace --locked`
Expected: PASS with no new warnings beyond the pre-existing `dag::erasure::gf256::mul_slice` dead-code warning.

- [ ] **Step 3: Confirm zero demo residue**

Run: `rg -n "l1_demo_blob_enabled|demo_blob_every|demo_blob_payload|demo_blobs_for_round|blob_custody_missing" apps/ crates/ config/`
Expected: zero hits.

- [ ] **Step 4: Confirm the spec is honored — file-by-file walk**

For each file listed in the §6 affected-files table of the spec [docs/superpowers/specs/2026-05-26-l1-driver-real-blob-attach-design.md](../specs/2026-05-26-l1-driver-real-blob-attach-design.md), open it and verify the change matches the spec description.

**Note on coverage:** The spec §7 suggests an extra assertion in `apps/node/tests/blob_custody_smoke.rs` confirming that one driver tick after submit yields a vertex with the submitted `blob_id`. The unit test `pending_blobs_attached_round_robin` added in Task 3 covers this exact behavior end-to-end (real BLS certs, 5 payloads, full drain), so this plan intentionally leaves `blob_custody_smoke.rs` scoped to "RPC submit → chunks persisted + custody available" and does not duplicate the assertion at the integration layer.

- [ ] **Step 5: Document the breaking change**

Append a one-line entry to a release notes file if the project keeps one (search for `CHANGELOG.md` or `RELEASE_NOTES.md` at the repo root; if neither exists, skip this step):

```
- BREAKING: Removed `[node].l1_demo_blob_enabled` and `[node].demo_blob_every_n_rounds`. Submit real blobs via the `lua_submitBlob` RPC; the L1 driver now anchors them to the next round's vertex headers automatically.
```

- [ ] **Step 6: Commit if a release note was added**

```bash
git add CHANGELOG.md  # or whichever file
git commit -m "docs: changelog entry for removed demo blob config"
```

---

## Out-of-scope notes (do not implement)

- **Persistent mempool.** The pending queue is in-memory. Node restart loses unanchored blobs. Submitters must retry. If a future plan needs durability, it gets its own spec.
- **Queue bound / backpressure.** Unbounded `VecDeque`. Devnet trust model accepts this; testnet/mainnet will need a follow-up.
- **Per-validator driver semantics.** This plan preserves the centralized devnet driver pattern. Splitting into per-validator drivers is a separate design.
- **`causal_set_rpc.rs:7` pre-existing `ValidatorId` import bug.** Out of scope. Track in a separate ticket.
