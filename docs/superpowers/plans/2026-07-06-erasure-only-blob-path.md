# Erasure-Only Blob Path (RS 4/8) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Reed–Solomon erasure coding the only blob custody/gossip path at RS 4/8 (rate 1/2, 32 KiB shards) and delete the sequential chunk mode end-to-end.

**Architecture:** Bump `n` to 8 everywhere the devnet RS parameters live. Make `BlobCustodyConfig` hold a mandatory `ErasureConfig` and strip the sequential branches from the node custody layer, with an explicit RPC error when a payload exceeds `k × data_shard_size`. Then delete the `ChunkPayload::Sequential` wire variant and every sequential helper from `crates/dag`, porting the dependent `crates/net` and test code to erasure shards.

**Tech Stack:** Rust workspace (cargo), borsh wire encoding, in-house RS codec (`crates/dag/src/erasure`), tokio, libp2p gossipsub.

**Spec:** [`docs/superpowers/specs/2026-07-06-erasure-only-blob-path-design.md`](../specs/2026-07-06-erasure-only-blob-path-design.md) (Approved).

## Global Constraints

- **Do NOT commit** — the user has asked for no commits this session; skip every commit step. Leave all changes in the working tree.
- Test baseline: the workspace has 4 known pre-existing failures (`node` lib `timer::tests::cancel_prevents_timer_event`, `node` test `blob_gossip_roundtrip`, `node` test `l1_distributed_smoke::genesis_proposal_plus_two_peer_partials_yield_verified_cert`, `consensus` test `vertex_cert_distributed::four_validators_certify_genesis_and_advance_to_round_one`). "Green" means no NEW failures beyond these 4.
- Max blob payload = `k × data_shard_size` (devnet: 4 × 32 KiB = 131072 bytes). Never hard-code 131072 in production code — always compute from config.
- Wire note: deleting the `Sequential` enum variant shifts the borsh tag of `Erasure` from 1 to 0. Accepted (pre-production); topic string `lua-dag/v1/blob-chunk` must NOT change.
- `apps/sim` is out of scope — do not modify it.
- Run commands from repo root `d:\1hoodlabs\lua-dag-consensus`.

---

### Task 1: RS parameters → 4/8 (rate 1/2)

**Files:**
- Modify: `crates/dag/src/erasure/config.rs:12-21`
- Modify: `apps/node/src/config_layers.rs` (`default_erasure_n`)
- Modify: `config/profiles/devnet.toml` (`erasure_n`)

**Interfaces:**
- Consumes: nothing.
- Produces: `ErasureConfig::devnet_default()` returns `{ k: 4, n: 8, data_shard_size: 32768 }` — later tasks and existing tests rely on `cfg.n` symbolically.

- [ ] **Step 1: Bump `devnet_default`**

In `crates/dag/src/erasure/config.rs` replace:

```rust
    /// Devnet defaults: 4 data + 2 parity shards, 32 KiB each.
    #[must_use]
    pub fn devnet_default() -> Self {
        Self {
            k: 4,
            n: 6,
            data_shard_size: 32 * 1024,
        }
    }
```

with:

```rust
    /// Devnet defaults: 4 data + 4 parity shards (rate 1/2), 32 KiB each.
    #[must_use]
    pub fn devnet_default() -> Self {
        Self {
            k: 4,
            n: 8,
            data_shard_size: 32 * 1024,
        }
    }
```

- [ ] **Step 2: Bump the node config default**

In `apps/node/src/config_layers.rs` replace:

```rust
fn default_erasure_n() -> u32 {
    6
}
```

with:

```rust
fn default_erasure_n() -> u32 {
    8
}
```

- [ ] **Step 3: Bump devnet profile**

In `config/profiles/devnet.toml` replace:

```toml
erasure_n = 6
```

with:

```toml
erasure_n = 8
```

- [ ] **Step 4: Run affected packages' tests**

Run: `cargo test -p dag -p node --locked --no-fail-fast`
Expected: no new failures vs baseline (the local `n: 6` configs inside `erasure_roundtrip.rs` / `rs.rs` unit tests are self-contained codec fixtures and still pass; every `devnet_default()` consumer asserts via `cfg.k`/`cfg.n`).

---

### Task 2: Node custody layer erasure-only + explicit RPC oversize error

Make `BlobCustodyConfig` erasure-mandatory, strip node-side sequential branches (leaving a transitional ignore-arm for the wire variant until Task 3 deletes it), and give `lua_submitBlob` a structured oversize error. TDD: the new submit-RPC test file is written first and drives the API change.

**Files:**
- Create: `apps/node/tests/submit_blob_rpc.rs`
- Modify: `apps/node/src/blob/mod.rs`
- Modify: `apps/node/src/rpc_server.rs:57` (dispatch), `:146-172` (`submit_blob`), `:169` (unit count call)
- Modify: `apps/node/tests/blob_status_rpc.rs:42-45`, `apps/node/tests/l1_distributed_smoke.rs:65-68`, `apps/node/tests/blob_custody_smoke.rs:44-47`, `apps/node/tests/erasure_recovery.rs:38-41`, `apps/node/tests/blob_gossip_roundtrip.rs`

**Interfaces:**
- Consumes: `ErasureConfig { k, n, data_shard_size }` and `ErasureConfig::padded_len() -> usize` (`crates/dag/src/erasure/config.rs`); `encode_shards(payload, &cfg)` fails when `payload.len() > cfg.padded_len()`.
- Produces (later tasks + tests rely on these exact shapes):
  - `pub struct BlobCustodyConfig { pub erasure: ErasureConfig }` (no `chunk_size`, no `Option`)
  - `BlobCustodyHandle::unit_count(&self) -> u32` (replaces `unit_count_for(size_bytes)`)
  - `BlobCustodyHandle::max_payload_bytes(&self) -> u64`
  - `pub async fn rpc_server::submit_blob(&Option<BlobCustodyHandle>, &serde_json::Value) -> serde_json::Value`, oversize response `{"error": "payload exceeds max blob size (<max> bytes)"}`

- [ ] **Step 1: Write the failing test file**

Create `apps/node/tests/submit_blob_rpc.rs`:

```rust
//! `lua_submitBlob` handler: erasure-only publish and explicit oversize rejection.

use std::sync::Arc;

use dag::erasure::ErasureConfig;
use node::{
    blob::{BlobCustody, BlobCustodyConfig, BlobCustodyHandle, RocksBlobStore},
    observability::metrics::Metrics,
    rpc_server::submit_blob,
};
use storage::{config::StorageConfig, db::Database};
use tokio::sync::mpsc;

fn spawn_custody(dir: &tempfile::TempDir) -> BlobCustodyHandle {
    let db = Arc::new(
        Database::open(&StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 16,
        })
        .unwrap(),
    );
    let store =
        Arc::new(RocksBlobStore::new(db)) as Arc<dyn dag::blob::store::BlobStore>;
    let (_chunks_tx, chunks_rx) = mpsc::channel(64);
    let (publish_tx, mut publish_rx) = mpsc::channel(256);
    tokio::spawn(async move { while publish_rx.recv().await.is_some() {} });
    BlobCustody::spawn(
        store,
        chunks_rx,
        publish_tx,
        BlobCustodyConfig {
            erasure: ErasureConfig {
                k: 4,
                n: 8,
                data_shard_size: 1024,
            },
        },
        Arc::new(Metrics::new().unwrap()),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn submit_rejects_payload_over_erasure_capacity() {
    let dir = tempfile::tempdir().unwrap();
    let custody = spawn_custody(&dir);
    // capacity = k * data_shard_size = 4096 bytes; send 5000.
    let params = serde_json::json!({ "payload_hex": hex::encode(vec![0xAAu8; 5000]) });
    let resp = submit_blob(&Some(custody), &params).await;
    assert_eq!(
        resp["error"],
        "payload exceeds max blob size (4096 bytes)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn submit_within_capacity_returns_blob_id_and_shard_count() {
    let dir = tempfile::tempdir().unwrap();
    let custody = spawn_custody(&dir);
    let params = serde_json::json!({ "payload_hex": hex::encode(vec![0xBBu8; 1500]) });
    let resp = submit_blob(&Some(custody), &params).await;
    assert!(resp["blob_id"].as_str().unwrap().starts_with("0x"));
    assert_eq!(resp["chunk_count"], 8);
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p node --test submit_blob_rpc --locked`
Expected: COMPILE FAIL — `submit_blob` is private, `BlobCustodyConfig` has no such field shape. (Feature missing, not a typo.)

- [ ] **Step 3: Rewrite the node custody config and handle**

In `apps/node/src/blob/mod.rs`:

(a) Imports (line 9-13) — drop the sequential helpers:

```rust
use dag::blob::chunk::{erasure_chunks, BlobChunk, ChunkPayload};
use dag::blob::commit::blob_id_from_payload;
use dag::blob::custody::CustodyLedger;
use dag::blob::store::BlobStore;
use dag::erasure::{encode_shards, rs_merkle_commitment, ErasureConfig};
```

(b) Config struct (lines 23-30):

```rust
/// Publish + custody configuration (erasure-only).
#[derive(Clone, Debug)]
pub struct BlobCustodyConfig {
    /// RS parameters; every blob is encoded to `n` shards of
    /// `data_shard_size` bytes, max payload `k * data_shard_size`.
    pub erasure: ErasureConfig,
}
```

(c) Replace `unit_count_for` and `blob_ref_commitment` (lines 58-77):

```rust
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
```

(d) In `publish_payload` (lines 99-107), replace the mode branch:

```rust
        let shards = encode_shards(&payload, &self.config.erasure)?;
        let chunks = erasure_chunks(blob_id, size_bytes, &shards);
```

(e) `register_chunk_in_ledger` (lines 197-212) — erasure config is no longer optional; ignore legacy sequential chunks until the wire variant is deleted in Task 3:

```rust
fn register_chunk_in_ledger(
    ledger: &mut CustodyLedger,
    chunk: &BlobChunk,
    erasure: ErasureConfig,
) {
    match &chunk.payload {
        // Transitional: erasure-only nodes drop legacy sequential chunks.
        // The variant itself is deleted in the follow-up dag change.
        ChunkPayload::Sequential { .. } => {}
        ChunkPayload::Erasure { n_shards, .. } => {
            ledger.register_erasure(chunk.blob_id, erasure, *n_shards, chunk.size_bytes);
        }
    }
}
```

(f) The two callers pass `self.config.erasure` (in `register_chunk_meta`, line 129-135) and `self.config.erasure` (in `BlobCustody::run`, line 187) instead of the old `Option` copy.

(g) The `spawn_handle()` unit-test helper (lines 242-245):

```rust
            BlobCustodyConfig {
                erasure: dag::erasure::ErasureConfig {
                    k: 4,
                    n: 8,
                    data_shard_size: 1024,
                },
            },
```

- [ ] **Step 4: RPC — public `submit_blob` with oversize error**

In `apps/node/src/rpc_server.rs` replace `submit_blob` (lines 146-172):

```rust
/// `lua_submitBlob` — publish a payload through blob custody. Rejects
/// payloads above the erasure capacity (`k * data_shard_size`) with an
/// explicit `error` field; returns `null` when custody is disabled.
pub async fn submit_blob(
    blob: &Option<BlobCustodyHandle>,
    params: &serde_json::Value,
) -> serde_json::Value {
    let Some(custody) = blob else {
        return serde_json::Value::Null;
    };
    let Some(hex_raw) = params.get("payload_hex").and_then(|v| v.as_str()) else {
        return serde_json::Value::Null;
    };
    let hex_str = hex_raw.strip_prefix("0x").unwrap_or(hex_raw);
    let Ok(payload) = hex::decode(hex_str) else {
        return serde_json::Value::Null;
    };
    let size_bytes = u64::try_from(payload.len()).unwrap_or(u64::MAX);
    let max = custody.max_payload_bytes();
    if size_bytes > max {
        return serde_json::json!({
            "error": format!("payload exceeds max blob size ({max} bytes)"),
        });
    }
    let chunk_count = custody.unit_count();
    match custody.publish_payload(payload).await {
        Ok(blob_id) => serde_json::json!({
            "blob_id": format!("0x{}", hex::encode(blob_id.0)),
            "chunk_count": chunk_count,
        }),
        Err(e) => {
            warn!(target: "node::rpc", error = %e, "lua_submitBlob failed");
            serde_json::Value::Null
        }
    }
}
```

(The dispatch at line 57 keeps calling `submit_blob(&state.blob, &req.params).await` — unchanged.)

- [ ] **Step 5: Port the four existing node test constructors**

Each currently constructs `BlobCustodyConfig { chunk_size: …, erasure: … }`.

`apps/node/tests/blob_status_rpc.rs` (lines 42-45) and `apps/node/tests/l1_distributed_smoke.rs` (lines 65-68) — small payloads (≤1.5 KiB):

```rust
            BlobCustodyConfig {
                erasure: dag::erasure::ErasureConfig {
                    k: 4,
                    n: 8,
                    data_shard_size: 1024,
                },
            },
```

`apps/node/tests/blob_custody_smoke.rs` (lines 44-47) — 100 KB payload, use devnet params (add `use dag::erasure::ErasureConfig;` to imports):

```rust
        BlobCustodyConfig {
            erasure: ErasureConfig::devnet_default(),
        },
```

`apps/node/tests/erasure_recovery.rs` (lines 38-41) — `cfg` already in scope:

```rust
        BlobCustodyConfig {
            erasure: cfg,
        },
```

- [ ] **Step 6: Port `apps/node/tests/blob_gossip_roundtrip.rs` to erasure shards**

Replace the import `use dag::blob::chunk::split_payload;` (line 6) with:

```rust
use dag::blob::chunk::erasure_chunks;
use dag::erasure::{encode_shards, ErasureConfig};
```

Replace the custody constructor (lines 85-88):

```rust
        BlobCustodyConfig {
            erasure: ErasureConfig::devnet_default(),
        },
```

Replace the chunk construction (lines 92-94):

```rust
    let payload = vec![0xCDu8; 100_000];
    let blob_id = dag::blob::commit::blob_id_from_payload(&payload);
    let cfg = ErasureConfig::devnet_default();
    let shards = encode_shards(&payload, &cfg).unwrap();
    let chunks = erasure_chunks(blob_id, payload.len() as u64, &shards);
```

(This test is a known pre-existing baseline failure — port it so it compiles; it stays on the known-failure list either way.)

- [ ] **Step 7: Verify GREEN**

Run: `cargo test -p node --test submit_blob_rpc --locked`
Expected: PASS — `2 passed`.

Run: `cargo test -p node --locked --no-fail-fast`
Expected: no new failures vs baseline (the 3 known `node` failures only).

---

### Task 3: Delete sequential support from `crates/dag` and `crates/net`

**Files:**
- Modify: `crates/dag/src/blob/chunk.rs` (full rewrite below)
- Modify: `crates/dag/src/blob/custody.rs`
- Modify: `crates/dag/src/blob/commit.rs:10-14` (delete `blob_commitment`)
- Modify: `apps/node/src/blob/mod.rs` (drop transitional arm), `apps/node/src/blob/rocks_store.rs:56-60`
- Modify: `crates/dag/tests/blob_chunk_roundtrip.rs` (rewrite), `crates/net/tests/blob_gossip_roundtrip.rs` (rewrite), `crates/net/src/gossip_wire.rs:290-301` (unit test), `crates/net/src/gossip/topics.rs:26,51` (comments)

**Interfaces:**
- Consumes: Task 2's erasure-only node layer (no node code references `Sequential` except the transitional arm removed here).
- Produces: `ChunkPayload` single-variant enum (`Erasure { index, n_shards, data }`); `CustodyLedger::register_erasure(blob_id, cfg, n_shards, size_bytes)` unchanged; `blob_commitment`, `split_payload`, `chunk_count`, `register_sequential`, `register_meta`, `CustodyKind` no longer exist anywhere.

- [ ] **Step 1: Rewrite `crates/dag/src/blob/chunk.rs`**

Replace the entire file with:

```rust
use borsh::{BorshDeserialize, BorshSerialize};
use types::primitives::BlobId;

/// Wire payload for one blob-chunk gossip message (07c erasure shard).
///
/// Single-variant enum kept for wire extensibility. Note: the legacy
/// `Sequential` variant was removed 2026-07-06; the borsh tag of
/// `Erasure` shifted from 1 to 0 (pre-production wire break, all nodes
/// upgrade together).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum ChunkPayload {
    /// Reed–Solomon erasure shard.
    Erasure {
        /// Shard index (`0..n-1`).
        index: u32,
        /// Total shard count.
        n_shards: u32,
        /// Shard bytes (fixed size per erasure config).
        data: Vec<u8>,
    },
}

/// One erasure shard gossiped on `blob-chunk`.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct BlobChunk {
    /// Content-addressed blob identifier.
    pub blob_id: BlobId,
    /// Full payload size in bytes (before padding).
    pub size_bytes: u64,
    /// Erasure shard body.
    pub payload: ChunkPayload,
}

impl BlobChunk {
    /// Shard index.
    #[must_use]
    pub fn index(&self) -> u32 {
        let ChunkPayload::Erasure { index, .. } = &self.payload;
        *index
    }

    /// Total shard count (`n_shards`).
    #[must_use]
    pub fn unit_count(&self) -> u32 {
        let ChunkPayload::Erasure { n_shards, .. } = &self.payload;
        *n_shards
    }

    /// Raw shard bytes.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        let ChunkPayload::Erasure { data, .. } = &self.payload;
        data
    }
}

/// Build erasure gossip chunks from RS-encoded shards (07c path).
#[must_use]
pub fn erasure_chunks(blob_id: BlobId, size_bytes: u64, shards: &[Vec<u8>]) -> Vec<BlobChunk> {
    let n_shards = u32::try_from(shards.len()).expect("shard count fits u32");
    shards
        .iter()
        .enumerate()
        .map(|(i, data)| BlobChunk {
            blob_id,
            size_bytes,
            payload: ChunkPayload::Erasure {
                index: u32::try_from(i).expect("index"),
                n_shards,
                data: data.clone(),
            },
        })
        .collect()
}
```

(`is_erasure()` is deleted — it is tautological now. Verify no callers: `rg "is_erasure" apps crates` must return nothing.)

- [ ] **Step 2: Simplify `crates/dag/src/blob/custody.rs`**

Delete `CustodyKind`, `register_sequential`, `register_meta`, `sequential_complete`; store the erasure config directly. Replace the file's struct/impl section (lines 8-105) with:

```rust
#[derive(Debug, Clone)]
struct BlobMeta {
    size_bytes: u64,
    cfg: ErasureConfig,
    received: HashSet<u32>,
}

/// In-memory custody ledger; completeness is verified against a [`BlobStore`].
#[derive(Debug, Default)]
pub struct CustodyLedger {
    meta: HashMap<BlobId, BlobMeta>,
    available: HashSet<BlobId>,
}

impl CustodyLedger {
    /// Register erasure shard expectations (07c).
    pub fn register_erasure(
        &mut self,
        blob_id: BlobId,
        cfg: ErasureConfig,
        _n_shards: u32,
        size_bytes: u64,
    ) {
        self.meta
            .entry(blob_id)
            .and_modify(|m| {
                m.size_bytes = size_bytes;
                m.cfg = cfg;
            })
            .or_insert(BlobMeta {
                size_bytes,
                cfg,
                received: HashSet::new(),
            });
    }

    /// Record that shard `index` was ingested; returns `true` when the blob
    /// newly transitions to locally available.
    pub fn note_chunk(&mut self, blob_id: &BlobId, index: u32, store: &dyn BlobStore) -> bool {
        if self.available.contains(blob_id) {
            return false;
        }
        let Some(meta) = self.meta.get_mut(blob_id) else {
            return false;
        };
        meta.received.insert(index);
        if erasure_available(blob_id, meta, store) {
            self.available.insert(*blob_id);
            true
        } else {
            false
        }
    }

    /// Whether the blob is locally readable.
    #[must_use]
    pub fn is_available(&self, blob_id: &BlobId) -> bool {
        self.available.contains(blob_id)
    }
}

fn erasure_available(blob_id: &BlobId, meta: &BlobMeta, store: &dyn BlobStore) -> bool {
    let cfg = &meta.cfg;
    if meta.received.len() < usize::try_from(cfg.k).unwrap_or(usize::MAX) {
        return false;
    }
    let mut present = Vec::new();
    for index in &meta.received {
        if let Ok(Some(data)) = store.get_chunk(blob_id, *index) {
            present.push((*index, data));
        }
    }
    if present.len() < usize::try_from(cfg.k).unwrap_or(usize::MAX) {
        return false;
    }
    decode_shards(&present, cfg, meta.size_bytes as usize).is_ok()
}
```

Keep the existing `use` lines (`decode_shards`, `ErasureConfig`, `BlobStore`, `HashMap`/`HashSet`, `BlobId`) — remove none except any that become unused.

- [ ] **Step 3: Delete `blob_commitment`**

In `crates/dag/src/blob/commit.rs` delete lines 10-14:

```rust
/// Payload commitment carried in [`types::dag::BlobRef`] (phase B).
#[must_use]
pub fn blob_commitment(payload: &[u8]) -> Hash32 {
    blake3_with_dst(dst::BLOB_COMMIT, payload)
}
```

Also remove the now-unused `Hash32` import if the compiler flags it (the `dst::BLOB_COMMIT` constant in `crates/crypto` stays — harmless).

- [ ] **Step 4: Drop the node transitional arm**

In `apps/node/src/blob/mod.rs`, `register_chunk_in_ledger` becomes:

```rust
fn register_chunk_in_ledger(
    ledger: &mut CustodyLedger,
    chunk: &BlobChunk,
    erasure: ErasureConfig,
) {
    let ChunkPayload::Erasure { n_shards, .. } = &chunk.payload;
    ledger.register_erasure(chunk.blob_id, erasure, *n_shards, chunk.size_bytes);
}
```

In `apps/node/src/blob/rocks_store.rs` (lines 56-60), `chunk_payload_bytes` becomes:

```rust
fn chunk_payload_bytes(chunk: &BlobChunk) -> &[u8] {
    let dag::blob::chunk::ChunkPayload::Erasure { data, .. } = &chunk.payload;
    data
}
```

- [ ] **Step 5: Rewrite `crates/dag/tests/blob_chunk_roundtrip.rs`**

Replace the imports (lines 1-9) and delete the three sequential tests (`blob_id_and_commitment_are_deterministic`, `split_100k_payload_with_64k_chunks`, `chunk_count_ceil_div`, `custody_marks_blob_available_when_all_chunks_present`), keeping `MemStore` and the erasure test. New file head:

```rust
use std::collections::HashMap;
use std::sync::Mutex;

use dag::blob::chunk::{BlobChunk, ChunkPayload};
use dag::blob::commit::blob_id_from_payload;
use dag::blob::custody::CustodyLedger;
use dag::blob::store::{BlobStore, StoreError};
use dag::erasure::{encode_shards, ErasureConfig};
use types::{dag::ChunkRef, primitives::BlobId};

#[test]
fn blob_id_is_deterministic() {
    let payload = b"rollup-batch-v0";
    assert_eq!(blob_id_from_payload(payload), blob_id_from_payload(payload));
}
```

(`MemStore` block lines 40-85 and `erasure_custody_available_with_k_data_shards_only` lines 105-129 stay byte-identical.)

- [ ] **Step 6: Port the `crates/net` wire tests and comments**

Replace `crates/net/tests/blob_gossip_roundtrip.rs` in full:

```rust
//! Blob shard encode/decode roundtrip on gossip wire (07c).

use dag::blob::chunk::erasure_chunks;
use dag::blob::commit::blob_id_from_payload;
use dag::erasure::{encode_shards, ErasureConfig};
use net::gossip::Topic;
use net::gossip_wire::{decode_blob_chunk, encode_blob_chunk};
use types::primitives::BlobId;

fn sample_chunks(payload: &[u8]) -> Vec<dag::blob::chunk::BlobChunk> {
    let cfg = ErasureConfig {
        k: 4,
        n: 8,
        data_shard_size: 32 * 1024,
    };
    let shards = encode_shards(payload, &cfg).unwrap();
    erasure_chunks(
        blob_id_from_payload(payload),
        payload.len() as u64,
        &shards,
    )
}

#[test]
fn blob_chunk_encode_decode_roundtrip() {
    let payload = vec![0xEFu8; 70_000];
    let chunk = sample_chunks(&payload).into_iter().next().unwrap();
    let (topic, bytes) = encode_blob_chunk(&chunk).unwrap();
    assert_eq!(topic, Topic::BlobChunk);
    let decoded = decode_blob_chunk(&topic.wire_name(), &bytes)
        .unwrap()
        .expect("blob chunk");
    assert_eq!(decoded, chunk);
}

#[test]
fn decode_returns_none_for_other_topics() {
    let got = decode_blob_chunk(Topic::MicroQc.wire_name().as_str(), &[]).unwrap();
    assert!(got.is_none());
}

#[test]
fn chunk_carries_blob_id_and_index() {
    let payload = b"rollup-batch-v0";
    let chunks = sample_chunks(payload);
    assert_eq!(chunks.len(), 8);
    assert_ne!(chunks[0].blob_id, BlobId([0; 32]));
    assert_eq!(chunks[0].index(), 0);
}
```

In `crates/net/src/gossip_wire.rs`, replace the unit test (lines 290-301):

```rust
    #[test]
    fn blob_chunk_encode_decode_roundtrip() {
        use dag::blob::chunk::erasure_chunks;
        use dag::erasure::{encode_shards, ErasureConfig};
        let payload = vec![0xEFu8; 70_000];
        let cfg = ErasureConfig {
            k: 4,
            n: 8,
            data_shard_size: 32 * 1024,
        };
        let shards = encode_shards(&payload, &cfg).unwrap();
        let chunk = erasure_chunks(
            dag::blob::commit::blob_id_from_payload(&payload),
            payload.len() as u64,
            &shards,
        )
        .into_iter()
        .next()
        .unwrap();
        let (topic, bytes) = encode_blob_chunk(&chunk).unwrap();
        assert_eq!(topic, Topic::BlobChunk);
        let decoded = decode_blob_chunk(&topic.wire_name(), &bytes)
            .unwrap()
            .expect("chunk");
        assert_eq!(decoded, chunk);
    }
```

In `crates/net/src/gossip/topics.rs`, change the two comments (lines 26 and 51) from `/// Sequential blob payload chunk stream (L1 07b).` to `/// Blob erasure shard stream (07c).` — the wire string `lua-dag/v1/blob-chunk` is untouched.

- [ ] **Step 7: Verify workspace**

Run: `rg "ChunkPayload::Sequential|split_payload|chunk_count\(|blob_commitment|register_sequential|sequential_complete|CustodyKind|is_erasure" apps crates`
Expected: no matches. (`unit_count` remains — that is the shard-count accessor.)

Run: `cargo test --workspace --locked --no-fail-fast`
Expected: no new failures vs the 4-failure baseline.

---

### Task 4: Remove the sequential config knobs

**Files:**
- Modify: `apps/node/src/config_layers.rs` (delete `l1_erasure_enabled`, `blob_chunk_size_bytes`, `default_blob_chunk_size`)
- Modify: `apps/node/src/runtime.rs` (`blob_custody_config`)
- Modify: `config/profiles/devnet.toml`

**Interfaces:**
- Consumes: Task 2's `BlobCustodyConfig { erasure }`.
- Produces: `NodeSection` without `l1_erasure_enabled` / `blob_chunk_size_bytes`; `erasure_k`, `erasure_n`, `erasure_data_shard_size_bytes` remain.

- [ ] **Step 1: Delete the two fields**

In `apps/node/src/config_layers.rs` delete:

```rust
    /// Fixed chunk size for blob payload splitting (07b).
    #[serde(default = "default_blob_chunk_size")]
    pub blob_chunk_size_bytes: u32,
    /// When true, publish RS erasure shards instead of sequential chunks (07c).
    #[serde(default)]
    pub l1_erasure_enabled: bool,
```

and the default fn:

```rust
fn default_blob_chunk_size() -> u32 {
    65_536
}
```

- [ ] **Step 2: Unconditional erasure config in runtime**

In `apps/node/src/runtime.rs` replace `blob_custody_config`:

```rust
fn blob_custody_config(node: &crate::config_layers::NodeSection) -> BlobCustodyConfig {
    BlobCustodyConfig {
        erasure: dag::erasure::ErasureConfig {
            k: node.erasure_k,
            n: node.erasure_n,
            data_shard_size: node.erasure_data_shard_size_bytes as usize,
        },
    }
}
```

- [ ] **Step 3: Clean `config/profiles/devnet.toml`**

Replace:

```toml
# Blob payload custody + chunk gossip (plan 07b).
l1_blob_custody_enabled = true
blob_chunk_size_bytes = 65536

# RS erasure-coded blob shards (plan 07c).
l1_erasure_enabled = true
erasure_k = 4
erasure_n = 8
erasure_data_shard_size_bytes = 32768
```

with:

```toml
# Blob payload custody + shard gossip (07b/07c, erasure-only).
l1_blob_custody_enabled = true

# RS erasure parameters: rate 1/2, max blob = erasure_k * shard size = 128 KiB.
erasure_k = 4
erasure_n = 8
erasure_data_shard_size_bytes = 32768
```

- [ ] **Step 4: Verify**

Run: `cargo test --workspace --locked --no-fail-fast`
Expected: no new failures vs baseline.

---

### Task 5: Architecture doc

**Files:**
- Modify: `docs/architecture/layer-1.md`

- [ ] **Step 1: Update the data-plane box**

Replace:

```
            RS["Erasure Coding<br/>RS Rate 1/2, 32KB chunks"]
```

with:

```
            RS["Erasure Coding<br/>RS 4/8 (Rate 1/2), 32KB shards<br/>max blob 128KB"]
```

---

### Task 6: Final verification sweep

**Files:** none.

- [ ] **Step 1: Residual sweep**

Run: `rg "l1_erasure_enabled|blob_chunk_size_bytes|chunk_size|Sequential" apps crates config`
Expected: no functional hits — allowed leftovers are only unrelated identifiers (verify each hit is not blob-sequential logic; `docs/` hits are fine).

- [ ] **Step 2: Full workspace test**

Run: `cargo test --workspace --locked --no-fail-fast`
Expected: 4 known baseline failures only; everything else passes, including the new `submit_blob_rpc` (2 tests) and the ported wire tests.

- [ ] **Step 3: Report**

Summarize: params now RS 4/8; sequential deleted from dag/net/node; oversize rejection live; docs reconciled. No commits made.
