# L1 Blob Payload + Custody + Chunk Wire (07b) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Store blob payload bytes on validators, gossip raw chunks on `blob-chunk`, let certified vertices carry `BlobRef`s, and track custody availability before treating a blob as readable — without erasure coding (07c) or SM changes.

**Architecture:** `crates/dag/blob/` owns chunking, payload commitment, and a `BlobStore` trait. `crates/storage` adds a `blob_chunk` column family and a Rocks-backed store impl. `apps/node` hosts a **BlobCustody** task (parallel to `L1Driver`) that ingests chunks from gossip + optional local publish RPC, updates a custody ledger, and exposes availability to the vertex builder. Vertex BLS verification (07a) covers the **header** (`BlobRef` list); custody is a separate host gate with metrics only.

**Tech Stack:** Rust 1.88, `borsh`, `blake3` (via `crypto`), `types`, `storage`/`rocksdb`, `net` gossip publish channel (same pattern as 06b-L1 `certified-vertex`).

**Spec:** [`docs/superpowers/specs/2026-05-23-l1-availability-dag-design.md`](../specs/2026-05-23-l1-availability-dag-design.md) §5 (Phase B).

**Prerequisite:** 07a landed (`crates/dag` cert/signing, `l1_real_vertex_certs`, orchestrator verify gate).

---

## Current gap

| Area | Today (post-07a) | Target |
|------|------------------|--------|
| Vertex blobs | Always `blobs: []` | Optional `Vec<BlobRef>` on devnet ticks |
| Payload bytes | None | Rocks `blob_chunk` CF |
| Wire | No blob topic | Gossip `lua-dag/v1/blob-chunk` |
| Commitment | N/A | `blake3_with_dst(BLOB_COMMIT, full_payload)` |
| Custody | N/A | Chunk-count threshold → blob available locally |
| Rollup publish | N/A | JSON-RPC `lua_submitBlob` (devnet) |

---

## Design decisions (lock-in)

| Topic | Decision |
|-------|----------|
| Chunking (07b) | **Sequential** fixed-size slices of raw payload (`index = 0..chunk_count-1`). Not erasure shards — that is 07c. |
| Default chunk size | `65536` bytes (64 KiB); override via `[node].blob_chunk_size_bytes` |
| `blob_id` | `BlobId(blake3_with_dst(BLOB_ID, full_payload).0)` — content-addressed |
| `BlobRef.commitment` (07b) | Same as `blake3_with_dst(BLOB_COMMIT, full_payload)` (KZG/RS root deferred to 07c) |
| `BlobRef.size_bytes` | Exact payload length in bytes |
| Store key | `blob_id ‖ chunk_index_be_u32` (36 bytes) → chunk bytes |
| SM `Action` | **No** new broadcast actions — blob gossip uses host `publish_tx` like L1 driver |
| SM `Event` | **No** new events — custody stays host-side; Bullshark unchanged |
| Verify split | Vertex cert verifies header; **custody** checks chunk completeness separately |
| Custody threshold | `chunk_count = ceil(size_bytes / chunk_size)`; available when store has all indices `0..chunk_count` |
| Ingress | Gossip → `BlobCustody::ingest_chunk` → `BlobStore::put_chunk`; orchestrator **not** involved |
| Devnet demo | Optional `[node].l1_demo_blob_enabled = true`: driver attaches one synthetic blob every `demo_blob_every_n_rounds` |
| Config gate | `[node].l1_blob_custody_enabled = true` in devnet; `false` preserves 07a-only behavior |

---

## File map

| File | Action |
|------|--------|
| `crates/crypto/src/hash.rs` | add `BLOB_ID`, `BLOB_COMMIT` DSTs |
| `crates/dag/src/lib.rs` | export `blob` module |
| `crates/dag/src/blob/mod.rs` | **CREATE** module root |
| `crates/dag/src/blob/chunk.rs` | **CREATE** chunk split + `BlobChunk` wire struct |
| `crates/dag/src/blob/commit.rs` | **CREATE** payload commitment + `blob_id_from_payload` |
| `crates/dag/src/blob/store.rs` | **CREATE** `BlobStore` trait + custody helpers |
| `crates/dag/src/blob/custody.rs` | **CREATE** availability tracker (in-memory + store probe) |
| `crates/storage/src/columns.rs` | add `BlobChunk` CF |
| `crates/storage/src/keys.rs` | add `blob_chunk(blob_id, index)` key encoder |
| `crates/storage/src/stores/blob_chunk_store.rs` | **CREATE** Rocks put/get/has |
| `crates/storage/src/stores/mod.rs` | export blob chunk store |
| `crates/net/src/gossip/topics.rs` | add `Topic::BlobChunk` |
| `crates/net/src/gossip_wire.rs` | encode/decode `BlobChunk`; inbound → host callback |
| `crates/net/src/swarm_runner.rs` | subscribe + publish `blob-chunk` (existing `publish_tx`) |
| `apps/node/src/blob/mod.rs` | **CREATE** `BlobCustody` service |
| `apps/node/src/blob/publish.rs` | **CREATE** split payload → gossip publish loop |
| `apps/node/src/config_layers.rs` | blob config fields |
| `config/profiles/devnet.toml` | enable blob custody + demo blob |
| `apps/node/src/l1/vertex_builder.rs` | optional `BlobRef` attachment |
| `apps/node/src/l1/driver.rs` | wait for custody before attaching demo blob |
| `apps/node/src/runtime.rs` | spawn `BlobCustody`, wire gossip ingress |
| `apps/node/src/rpc_server.rs` | `lua_submitBlob` (devnet) |
| `apps/node/src/observability/metrics.rs` | chunk/custody counters |
| `crates/dag/tests/blob_chunk_roundtrip.rs` | **CREATE** split + commitment tests |
| `apps/node/tests/blob_custody_smoke.rs` | **CREATE** submit → chunks → available |
| `apps/node/tests/blob_gossip_roundtrip.rs` | **CREATE** two-node chunk gossip |
| `docs/superpowers/specs/2026-05-23-l1-availability-dag-design.md` | status bump Phase B plan-ready → landed |

---

### Task 1: DSTs + payload identity

**Files:**
- Modify: `crates/crypto/src/hash.rs`
- Create: `crates/dag/src/blob/commit.rs`
- Create: `crates/dag/src/blob/mod.rs`
- Modify: `crates/dag/src/lib.rs`

- [ ] **Step 1: Add DSTs**

```rust
    /// Content-addressed blob identifier (L1 07b).
    pub const BLOB_ID: &[u8] = b"lua-dag/v1/blob-id";
    /// Payload commitment for BlobRef (L1 07b; RS/KZG deferred to 07c).
    pub const BLOB_COMMIT: &[u8] = b"lua-dag/v1/blob-commit";
```

- [ ] **Step 2: Failing test** in `crates/dag/tests/blob_chunk_roundtrip.rs`:

```rust
use dag::blob::commit::{blob_commitment, blob_id_from_payload};

#[test]
fn blob_id_and_commitment_are_deterministic() {
    let payload = b"rollup-batch-v0";
    let id1 = blob_id_from_payload(payload);
    let id2 = blob_id_from_payload(payload);
    assert_eq!(id1, id2);
    assert_ne!(blob_commitment(payload), id1.0.into()); // different DSTs
}
```

- [ ] **Step 3: Implement `commit.rs`**

```rust
use crypto::hash::{blake3_with_dst, dst};
use types::{crypto_types::Hash32, primitives::BlobId};

#[must_use]
pub fn blob_id_from_payload(payload: &[u8]) -> BlobId {
    BlobId(blake3_with_dst(dst::BLOB_ID, payload).0)
}

#[must_use]
pub fn blob_commitment(payload: &[u8]) -> Hash32 {
    blake3_with_dst(dst::BLOB_COMMIT, payload)
}
```

- [ ] **Step 4: Run**

Run: `cargo test -p dag blob_id_and_commitment --locked`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/crypto/src/hash.rs crates/dag/
git commit -m "feat(dag): blob payload id and commitment (07b)"
```

---

### Task 2: Sequential chunking + wire type

**Files:**
- Create: `crates/dag/src/blob/chunk.rs`
- Modify: `crates/dag/src/blob/mod.rs`

- [ ] **Step 1: Failing test**

```rust
use dag::blob::chunk::{chunk_count, split_payload, BlobChunk};

#[test]
fn split_100k_payload_with_64k_chunks() {
    let payload = vec![0xABu8; 100_000];
    let chunks = split_payload(&payload, 65_536);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].index, 0);
    assert_eq!(chunks[1].index, 1);
    assert_eq!(chunks[0].data.len(), 65_536);
    assert_eq!(chunks[1].data.len(), 100_000 - 65_536);
    let rebuilt: Vec<u8> = chunks.iter().flat_map(|c| c.data.clone()).collect();
    assert_eq!(rebuilt, payload);
}
```

- [ ] **Step 2: Implement**

```rust
use borsh::{BorshDeserialize, BorshSerialize};
use types::primitives::BlobId;

/// One sequential payload slice gossiped on `blob-chunk`.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct BlobChunk {
    pub blob_id: BlobId,
    pub index: u32,
    pub total_chunks: u32,
    pub size_bytes: u64,
    pub data: Vec<u8>,
}

#[must_use]
pub fn chunk_count(size_bytes: u64, chunk_size: u32) -> u32 {
    let cs = u64::from(chunk_size);
    u32::try_from(size_bytes.div_ceil(cs)).unwrap_or(u32::MAX)
}

pub fn split_payload(payload: &[u8], chunk_size: u32) -> Vec<BlobChunk> {
    let blob_id = super::commit::blob_id_from_payload(payload);
    let size_bytes = u64::try_from(payload.len()).expect("payload fits u64");
    let total = chunk_count(size_bytes, chunk_size);
    payload
        .chunks(chunk_size as usize)
        .enumerate()
        .map(|(i, data)| BlobChunk {
            blob_id,
            index: u32::try_from(i).expect("index"),
            total_chunks: total,
            size_bytes,
            data: data.to_vec(),
        })
        .collect()
}
```

- [ ] **Step 3: Run**

Run: `cargo test -p dag split_100k --locked`  
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(dag): sequential blob chunk split + wire type (07b)"
```

---

### Task 3: `BlobStore` trait + custody tracker

**Files:**
- Create: `crates/dag/src/blob/store.rs`
- Create: `crates/dag/src/blob/custody.rs`

- [ ] **Step 1: Trait**

```rust
use types::primitives::BlobId;
use crate::blob::chunk::BlobChunk;

pub trait BlobStore: Send + Sync {
    fn put_chunk(&self, chunk: &BlobChunk) -> Result<(), StoreError>;
    fn get_chunk(&self, blob_id: &BlobId, index: u32) -> Result<Option<Vec<u8>>, StoreError>;
    fn has_chunk(&self, blob_id: &BlobId, index: u32) -> Result<bool, StoreError>;
}
```

- [ ] **Step 2: Custody tracker**

```rust
impl CustodyLedger {
    pub fn register_meta(&mut self, blob_id: BlobId, total_chunks: u32, size_bytes: u64);
    pub fn note_chunk(&mut self, blob_id: &BlobId, index: u32, store: &dyn BlobStore) -> bool;
    pub fn is_available(&self, blob_id: &BlobId) -> bool;
}
```

`note_chunk` returns `true` when blob transitions to available (all indices present).

- [ ] **Step 3: Unit test** with in-memory `HashMap` store mock in `crates/dag/tests/blob_chunk_roundtrip.rs`.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(dag): BlobStore trait and custody ledger (07b)"
```

---

### Task 4: Rocks `blob_chunk` column family

**Files:**
- Modify: `crates/storage/src/columns.rs`, `keys.rs`
- Create: `crates/storage/src/stores/blob_chunk_store.rs`
- Modify: `crates/storage/src/stores/mod.rs`, `db.rs` tests

- [ ] **Step 1: Column family**

```rust
    /// `(blob_id, chunk_index) -> chunk bytes`.
    BlobChunk,
```

Wire name: `"blob_chunk"`.

- [ ] **Step 2: Key encoder**

```rust
#[must_use]
pub fn blob_chunk(blob_id: &BlobId, index: u32) -> [u8; 36] {
    let mut out = [0u8; 36];
    out[..32].copy_from_slice(&blob_id.0);
    out[32..].copy_from_slice(&index.to_be_bytes());
    out
}
```

- [ ] **Step 3: Store functions** `put_chunk`, `get_chunk`, `has_chunk` using Borsh-encoded `BlobChunk` or raw bytes only (store raw `data` + metadata sidecar in value: borsh `{total_chunks, size_bytes}` + bytes).

- [ ] **Step 4: Test** `crates/storage/tests/blob_chunk_store.rs` roundtrip.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(storage): blob_chunk column family (07b)"
```

---

### Task 5: Gossip topic + wire codec

**Files:**
- Modify: `crates/net/src/gossip/topics.rs`
- Modify: `crates/net/src/gossip_wire.rs`
- Modify: `crates/net/src/swarm_runner.rs` (subscribe mesh)

- [ ] **Step 1: Topic**

```rust
    pub const BLOB_CHUNK: &str = "lua-dag/v1/blob-chunk";
// Topic::BlobChunk variant + from_wire_name arm
```

- [ ] **Step 2: Encode/decode helpers**

```rust
pub fn encode_blob_chunk(chunk: &BlobChunk) -> Result<(Topic, Vec<u8>)> {
    Ok((Topic::BlobChunk, borsh::to_vec(chunk)?))
}

pub fn decode_blob_chunk(topic: &str, data: &[u8]) -> Result<Option<BlobChunk>> {
    if Topic::from_wire_name(topic) != Some(Topic::BlobChunk) {
        return Ok(None);
    }
    Ok(Some(borsh::from_slice(data)?))
}
```

- [ ] **Step 3: Swarm ingress** — on `BlobChunk` topic, send to dedicated `mpsc::Sender<BlobChunk>` (parallel to consensus `events_tx`). **Do not** map to `consensus::Event`.

- [ ] **Step 4: Test** in `crates/net/tests/blob_gossip_roundtrip.rs` (two loopback swarms, mirror `l1_gossip_roundtrip.rs`).

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(net): blob-chunk gossip topic and codec (07b)"
```

---

### Task 6: Node `BlobCustody` service + runtime wiring

**Files:**
- Create: `apps/node/src/blob/mod.rs`, `publish.rs`
- Modify: `apps/node/src/runtime.rs`, `lib.rs`
- Modify: `apps/node/src/config_layers.rs`, `config/profiles/devnet.toml`

- [ ] **Step 1: Config**

```toml
[node]
l1_blob_custody_enabled = true
blob_chunk_size_bytes = 65536
l1_demo_blob_enabled = true
demo_blob_every_n_rounds = 8
```

```rust
    #[serde(default)]
    pub l1_blob_custody_enabled: bool,
    #[serde(default = "default_blob_chunk_size")]
    pub blob_chunk_size_bytes: u32,
    #[serde(default)]
    pub l1_demo_blob_enabled: bool,
    #[serde(default = "default_demo_blob_every")]
    pub demo_blob_every_n_rounds: u64,
```

- [ ] **Step 2: `BlobCustody` task**

```rust
pub struct BlobCustody {
    store: Arc<dyn BlobStore>,
    ledger: CustodyLedger,
    chunks_rx: mpsc::Receiver<BlobChunk>,
    publish_tx: mpsc::Sender<(Topic, Vec<u8>)>,
    metrics: Arc<Metrics>,
}

impl BlobCustody {
    pub async fn run(mut self) {
        while let Some(chunk) = self.chunks_rx.recv().await {
            let _ = self.store.put_chunk(&chunk);
            if self.ledger.note_chunk(&chunk.blob_id, chunk.index, &*self.store) {
                self.metrics.blob_available.inc();
            }
        }
    }

    pub async fn publish_payload(&self, payload: Vec<u8>) -> Result<BlobId> {
        for chunk in split_payload(&payload, self.chunk_size) {
            let (topic, bytes) = encode_blob_chunk(&chunk)?;
            self.publish_tx.send((topic, bytes)).await?;
            self.store.put_chunk(&chunk)?;
            self.ledger.register_meta(chunk.blob_id, chunk.total_chunks, chunk.size_bytes);
            self.ledger.note_chunk(&chunk.blob_id, chunk.index, &*self.store);
        }
        Ok(blob_id_from_payload(&payload))
    }
}
```

- [ ] **Step 3: Runtime** — spawn when `l1_blob_custody_enabled`; wire swarm `chunks_tx` fan-in.

- [ ] **Step 4: Metrics** — `blob_chunks_received_total`, `blob_chunks_published_total`, `blob_available_total`, `blob_chunk_rejected_total`.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(node): BlobCustody service and runtime wiring (07b)"
```

---

### Task 7: Vertices carry `BlobRef` + devnet demo path

**Files:**
- Modify: `apps/node/src/l1/vertex_builder.rs`, `driver.rs`

- [ ] **Step 1: Extend builder**

```rust
pub fn build_certified_vertex_with_blobs(
    round: u64,
    author: ValidatorId,
    parent_hash: Option<Hash32>,
    real_certs: bool,
    valset: &ValidatorSet,
    blobs: Vec<BlobRef>,
) -> CertifiedVertex
```

When `real_certs`, include `blobs` in signing root (already part of 07a `signing_bytes`).

- [ ] **Step 2: Driver demo** — every `demo_blob_every_n_rounds`, if custody reports demo blob available, attach:

```rust
BlobRef {
    blob_id,
    commitment: blob_commitment(&payload),
    size_bytes: payload.len() as u64,
}
```

Generate deterministic demo payload: `blake3(b"demo-blob", round.to_be_bytes())` padded to >1 chunk for multi-chunk test.

- [ ] **Step 3: Custody gate** — if vertex lists `BlobRef` and blob not available locally, log + metric `blob_custody_missing`; still allow vertex cert verify (header valid) but skip attach on publish path for incomplete blobs.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(node): attach BlobRef to certified vertices (07b)"
```

---

### Task 8: Rollup-facing RPC (devnet)

**Files:**
- Modify: `apps/node/src/rpc_server.rs`
- Modify: `apps/node/src/runtime.rs` (share `BlobCustody` handle)

- [ ] **Step 1: Method `lua_submitBlob`**

Params:

```json
{ "payload_hex": "deadbeef..." }
```

Response:

```json
{ "blob_id": "...", "chunk_count": 3 }
```

- [ ] **Step 2: Hex decode → `publish_payload` → return `blob_id`.**

- [ ] **Step 3: Test** `apps/node/tests/blob_custody_smoke.rs` — RPC submit, assert custody available + chunks in Rocks.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(node): lua_submitBlob RPC for devnet rollup (07b)"
```

---

### Task 9: Integration tests + docs

- [ ] **Step 1:** `apps/node/tests/blob_gossip_roundtrip.rs` — node A publishes, node B receives all chunks, `is_available`.

- [ ] **Step 2:** Regression

Run: `cargo test -p dag -p storage -p net -p node blob --locked`  
Run: `cargo test -p sim -p consensus --locked` (07b must not break L2)

- [ ] **Step 3:** Update spec status → **Phase B (07b) landed**

- [ ] **Step 4: Commit**

```bash
git commit -m "test(node): blob custody smoke + gossip roundtrip (07b)"
```

---

## Done — 07b acceptance criteria

- `lua_submitBlob` on devnet splits payload, stores chunks, gossips `blob-chunk`.
- Peers ingest chunks into Rocks `blob_chunk` CF; custody ledger marks blob available when complete.
- Certified vertices may include `BlobRef { blob_id, commitment, size_bytes }`; 07a cert verify still passes.
- Optional demo blob attached by `L1Driver` on schedule when custody enabled.
- With `l1_blob_custody_enabled = false`, behavior identical to 07a-only.

**Non-goals (explicit):**

- Erasure coding / RS parity shards (07c)
- KZG commitments
- DA slashing evidence
- SM `Action` / `Event` changes
- Cross-validator blob repair protocol

**Next:** [`2026-05-23-07c-l1-erasure-da.md`](./2026-05-23-07c-l1-erasure-da.md)

---

## Execution hand-off

Plan complete — saved `docs/superpowers/plans/2026-05-23-07b-l1-blob-custody.md`.

Execution options:

1. **Subagent-driven (recommended)** — fresh subagent per task, checkpoint reviews.
2. **Inline execution** — run tasks sequentially in-session with checkpoints.

Tell me **`1`** or **`2`** when starting implementation.
