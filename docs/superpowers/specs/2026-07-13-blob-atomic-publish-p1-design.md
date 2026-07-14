# Design: Blob atomic publish + crash-safe pending (Phase P1)

**Date:** 2026-07-13 (rev 4.1 — 2026-07-14)
**Status:** Approved for implementation (rev 4.1 — round-4 patch)
**Audience:** Contributors editing `apps/node/src/blob/`, `crates/storage/`, and `crates/consensus/src/ports/`
**Relations:**
- [`2026-05-26-l1-driver-real-blob-attach-design.md`](2026-05-26-l1-driver-real-blob-attach-design.md) — introduced in-memory `pending` queue (explicit non-goal: persistent mempool)
- [`2026-06-04-distributed-vertex-certificate-design.md`](2026-06-04-distributed-vertex-certificate-design.md) — `drain_pending()` on author loop only
- [`2026-07-06-erasure-only-blob-path-design.md`](2026-07-06-erasure-only-blob-path-design.md) — erasure-only publish path

**Changelog:**
- **Rev 2:** `mark_attached` after vertex seal; boot sequence; ledger rehydrate; idempotent enqueue; `confirm_attached` port; ~900 LOC
- **Rev 3:** Boot re-enqueue; duplicate-attach idempotency; confirm timing; confirm fail = log+metric; dependency boundary
- **Rev 4:** Drain-time DB re-check (skip Attached); lightweight ledger rehydrate; at-least-once attach semantics; expanded tests; split orphan metrics; `PendingQueue` struct
- **Rev 4.1:** `boot_sync_done` gate; propose ordering `mark_attached` before broadcast; drain pop-in-lock / DB-filter-outside-lock; orphan definition; 3 tests

---

## 1. Problem

`BlobCustodyHandle::publish_payload()` (apps/node/src/blob/mod.rs) has two durability gaps that block production-shaped blob attach:

### G3 — Non-atomic publish loop

Current flow per shard:

```
gossip → put_chunk(RocksDB) → ledger update
```

If the loop fails on shard *i* (channel closed, disk error, panic):

| Artifact | State |
|---|---|
| Shards `0..i-1` | Persisted in `BlobChunk` CF |
| Shards `i..n-1` | Missing |
| `enqueue_pending(BlobRef)` | **Not called** |
| RPC caller | Receives `Err`, must retry |

Partial chunks remain in RocksDB with no `BlobRef` ever reaching a vertex header. The 2026-05-26 design doc documents this as acceptable for devnet; it is **not** acceptable for production retry semantics.

### G2 — Pending queue is RAM-only

`pending: Arc<Mutex<VecDeque<BlobRef>>>` is initialized empty in `BlobCustody::spawn` and never persisted.

If the node crashes after a successful `publish_payload()` but before `AuthorLoop` calls `drain_pending()`:

| Artifact | State |
|---|---|
| All shards | In RocksDB |
| `BlobRef` | **Lost** (RAM) |
| After restart | Pending queue empty → blob never attached to L1 |

The 2026-05-26 spec explicitly listed "Persistent mempool" as a **non-goal**. This design **revises** that decision for the publish→attach path only: durability is required so submitters do not need to resubmit after benign restarts.

### Out of scope for P1

| Gap | Deferred to |
|---|---|
| G1 — duplicate payload submit / full idempotent RPC | Phase P2 |
| G4 — RS-Merkle per-shard ingest verify | Phase P3–P4 |
| Gossip retry worker | Phase P2+ (best-effort gossip in RPC thread remains for P1) |
| Orphan shard janitor (delete pre-P1 partial writes) | Phase P2+ |
| Lease/ack after vertex cert | Mainnet (P5) |

---

## 2. Goals

1. **Atomic local publish:** all `n` erasure shards + publish metadata commit in a single RocksDB `WriteBatch`, or none.
2. **Crash-safe pending attach:** a successfully published blob survives restart and is re-enqueued for `AuthorLoop` drain.
3. **Pending queue remains a cache:** RocksDB `BlobPublish` CF is source of truth; `VecDeque` is a derived runtime queue.
4. **Attach path uses `PublishRecord`, not ledger:** `BlobRef` in `PublishRecord` is sufficient for vertex attach; `CustodyLedger` is derived state.
5. **At-least-once attach:** attach is retried until `mark_attached` succeeds; not exactly-once (duplicate vertex proposals acceptable in P1).
6. **Preserve architectural invariants:**
   - Only `publish_payload()` enqueues pending (gossip ingest does not).
   - `BlobRef` wire format unchanged.
   - `lua_submitBlob` RPC response shape unchanged (see §4.11 for gossip-failure semantics).
7. **Testnet-safe migration:** new column family auto-created on DB open; no rewrite of existing `BlobChunk` rows.

## 3. Non-goals

- Full idempotent dedup on duplicate payload submit (P2).
- Moving gossip off the RPC task (P2: enqueue before gossip async).
- Changing erasure parameters, chunk wire format, or `BlobStatus` (L3 tier lifecycle).
- Deleting pre-P1 orphan shards (log + metric only in P1).
- Exactly-once vertex attach (P5 lease/ack).

---

## 4. Architecture

### 4.1 Source of truth

```
RocksDB BlobPublish CF          PendingQueue (RAM)
  blob_id → PublishRecord           VecDeque + HashSet (single mutex)
  state: Ready | Attached           runtime cache for AuthorLoop

RocksDB BlobChunk CF            CustodyLedger (RAM)
  all n shards per blob               derived; lightweight rehydrate on boot
```

**Invariants:**

1. Every `BlobRef` in the pending queue MUST have `PublishRecord { state: Ready }` in RocksDB.
2. **`Ready` means:** all `n` chunks durable in `BlobChunk` CF + publish record durable + **not yet attached to L1**. Gossip may or may not have succeeded. **`Ready` ≠ DA-guaranteed** (peers may not have chunks; no Narwhal-style availability cert in P1).
3. **`Attached` means:** `BlobRef` was included in a locally sealed **and signed** vertex proposal for this node. Does **not** imply DAG commit/finality or DA-guaranteed (P5 lease/ack deferred).
4. **`PublishRecord { Ready }` MUST reference existing chunks:** boot validates `has_chunk(blob_id, 0..n-1)`; corrupt rows → warn + metric + skip (no panic).
5. **Duplicate attach (P1):** The same `blob_id` MAY appear in more than one local vertex proposal after crash recovery. Attachment is **idempotent** — consensus/execution treats duplicate `BlobRef` as no-op on re-apply (first occurrence wins).
6. **Attach semantics:** **at-least-once**, not exactly-once. `confirm_attached` failure does not invalidate the sealed vertex; blob stays `Ready` until `mark_attached` succeeds.

### 4.2 Publish state machine (minimal)

```rust
#[repr(u8)]
enum PublishState {
    Ready    = 0,  // locally durable, awaiting vertex attach
    Attached = 1,  // included in a sealed local vertex proposal
}
```

No `Publishing` state: RocksDB `WriteBatch` commit is atomic; a blob is either absent or `Ready`.

Transition table:

| From | Event | To | Result |
|---|---|---|---|
| (none) | `WriteBatch` commit success | `Ready` | — |
| `Ready` | `mark_attached()` after vertex sealed+signed | `Attached` | `Ok(())` |
| `Attached` | `mark_attached()` again (same blob) | `Attached` | **`Ok(())` idempotent no-op** |
| `Ready` | `mark_attached()` when DB I/O fails | `Ready` | `Err` — retry next round |
| `Attached` | — | terminal for P1 | — |

**Critical:** `Attached` MUST NOT be written before the vertex proposal containing the `BlobRef` is successfully sealed and signed. Writing `Attached` before attach causes silent blob loss on restart (worse than duplicate attach).

### 4.3 New storage: `BlobPublish` column family

Add to `crates/storage/src/columns.rs`:

```rust
/// `blob_id -> PublishRecord` (local publish/attach lifecycle).
BlobPublish,
// wire name: "blob_publish"
```

**Key:** `keys::blob_id(&BlobId)` — 32 bytes.

**Value:** borsh-encoded:

```rust
#[derive(BorshSerialize, BorshDeserialize)]
pub struct PublishRecord {
    pub state: u8,           // PublishState discriminant (enum, not bool — reserve for P2)
    pub blob_ref: BlobRef,   // { blob_id, commitment, size_bytes } — authoritative for attach
}
```

New module: `crates/storage/src/stores/blob_publish_store.rs`

| Function | Purpose |
|---|---|
| `put_ready_batch(batch, blob_id, record)` | Add to WriteBatch |
| `put_attached(db, blob_id) -> Result<()>` | Ready→Attached; **Attached→Attached = `Ok(())` no-op** |
| `get(db, blob_id) -> Result<Option<PublishRecord>>` | Lookup / drain-time re-check |
| `is_attached(db, blob_id) -> Result<bool>` | Fast drain filter |
| `scan_ready(db) -> Result<Vec<BlobRef>>` | Boot recovery |

Add `Database::scan_cf(cf) -> impl Iterator<Item = Result<(Vec<u8>, Vec<u8>)>>` for boot scan and orphan detection.

**Do not reuse `BlobStatus` CF** — that tracks L3 consensus tiers (`Accepted`…`EpochFinalized`), not local publish lifecycle.

**API placement:** `publish_blob_atomic`, `mark_attached`, `scan_ready_blobs`, `is_attached` live on **`RocksBlobStore` concrete type only**. The `BlobStore` trait is **not extended** until P2.

### 4.4 Atomic write boundary

**Inside WriteBatch (atomic):**

- All `n` rows in `BlobChunk` CF
- One `PublishRecord { Ready, blob_ref }` row in `BlobPublish` CF

**Outside WriteBatch (derived / best-effort):**

- `CustodyLedger` RAM updates
- Gossip `publish_tx.send`
- `PendingQueue` enqueue

This boundary MUST NOT be expanded in P1. Ledger is rebuildable from `PublishRecord` + chunk key presence (not payload reads).

### 4.5 Atomic commit implementation

Extend `crates/storage/src/stores/blob_chunk_store.rs`:

```rust
pub fn put_batch(
    batch: &mut WriteBatch,
    db: &Database,
    blob_id: &BlobId,
    index: u32,
    total_chunks: u32,
    size_bytes: u64,
    data: &[u8],
) -> Result<()>
```

Commit path in `apps/node/src/blob/rocks_store.rs`:

```rust
impl RocksBlobStore {
    pub fn publish_blob_atomic(
        &self,
        chunks: &[BlobChunk],
        record: PublishRecord,
    ) -> Result<(), StoreError> {
        let mut batch = WriteBatch::default();
        for chunk in chunks { blob_chunk_store::put_batch(&mut batch, ...)?; }
        blob_publish_store::put_ready_batch(&mut batch, &record.blob_ref.blob_id, &record)?;
        wal::apply(&self.db, batch).map_err(...)?;
        Ok(())
    }
}
```

Single RocksDB write; all-or-nothing.

### 4.6 Revised `publish_payload` flow

```
lua_submitBlob
    │
    ▼
publish_payload(payload)          [only after boot recovery complete — §4.7]
    │
    ├─ blob_id = hash(payload)
    ├─ shards = encode_shards(payload, erasure)
    ├─ chunks = erasure_chunks(...)
    ├─ record = PublishRecord { Ready, BlobRef { ... } }
    │
    ├─ store.publish_blob_atomic(&chunks, record)     ◀── G3
    │
    ├─ enqueue_pending(blob_ref)                      ◀── idempotent; §4.8 (before gossip)
    │
    ├─ for chunk in chunks:
    │     register_chunk_meta + ledger.note_chunk     ◀── derived; best-effort
    │
    ├─ for chunk in chunks:
    │     publish_tx.send(gossip)                     ◀── may fail; see §4.11
    │
    └─ Ok(blob_id)
```

**P1 note:** Enqueue immediately after atomic commit so proposer can pick up the blob without waiting for gossip. Gossip remains best-effort in RPC thread (P2 moves to background worker).

Attach path reads `BlobRef` from `PublishRecord` / drained pending — **does not require ledger entry**.

### 4.7 Boot sequence (pinned)

Recovery runs **synchronously** inside `BlobCustody::spawn` before the handle is returned. Runtime does not expose the handle to RPC until `spawn` returns.

Boot completes **both** pending rebuild and ledger rehydrate before returning the handle. Internal step order is flexible unless a step depends on prior output; recommended order below.

```
Open DB (already open; passed into spawn)
    │
    ├─ scan BlobPublish CF → collect Ready records (skip Attached)
    │
    ├─ for each Ready record:
    │     validate has_chunk(blob_id, 0..n-1)
    │       OK  → keep
    │       FAIL → warn + blob_ready_without_chunks_total++ + skip
    │
    ├─ lightweight ledger rehydrate for valid Ready blobs (§4.7.1)
    │
    ├─ re-enqueue valid Ready blob_refs into PendingQueue (idempotent §4.8)
    │     ◀── crash after drain but before confirm_attached → re-queued here
    │
    ├─ orphan scan:
    │     BlobChunk keys without BlobPublish row → warn + blob_orphan_chunk_total++
    │     (orphan = chunk row exists with no matching PublishRecord; should not occur post-P1 except pre-migration debris)
    │
    ├─ boot_sync_done.store(true)                     ◀── §4.7.2
    │
    └─ return BlobCustodyHandle (boot complete)
```

**Assumption (P1 testnet):** Ready count ≪ 10⁴; full CF scan acceptable. Large backlog may delay startup but is acceptable in P1. P2: prefix index / paginated scan.

#### 4.7.2 Boot sync gate (`boot_sync_done`)

`BlobCustodyHandle` holds `boot_sync_done: Arc<AtomicBool>`, initially `false`, set `true` only after boot recovery completes (last step before return from `spawn`).

- `publish_payload` MUST NOT run until `boot_sync_done` is true (assert in debug; `Err` or brief spin in release — pick one at implement time).
- `propose_round` MUST NOT call `pending_blobs.drain()` until `boot_sync_done` is true. Simplest: `CustodyPendingBlobs` checks the flag and returns empty drain until boot completes.

This prevents race between boot re-enqueue and in-flight propose during the narrow startup window. `PendingQueue::enqueue` dedup (§4.8) remains the second line of defense.

**Expected race (safe after boot):** boot re-enqueue may overlap with concurrent `mark_attached` from another thread. Boot may enqueue a blob that becomes `Attached` before drain. Safe because `drain_pending` re-checks `is_attached` in RocksDB (§4.9).

#### 4.7.1 Lightweight ledger rehydrate

**Do NOT read chunk payload bytes on boot.** Use metadata already in `PublishRecord.blob_ref` and erasure config from node:

```rust
for record in ready_records {
    ledger.register_erasure(
        record.blob_ref.blob_id,
        erasure_config,
        erasure_config.n,
        record.blob_ref.size_bytes,
    );
    for index in 0..erasure_config.n {
        if store.has_chunk(&blob_id, index)? {
            ledger.note_chunk_index(blob_id, index);  // metadata only, no get_chunk
        }
    }
}
```

If `note_chunk` requires store reads today, add a lightweight `note_chunk_present(blob_id, index)` on ledger for boot path only.

### 4.8 PendingQueue (idempotent enqueue)

Wrap queue + dedup set in one struct under a single mutex to preserve invariant **queue membership == HashSet membership**:

```rust
struct PendingQueue {
    queue: VecDeque<BlobRef>,
    ids: HashSet<BlobId>,
}

impl PendingQueue {
    fn enqueue(&mut self, blob_ref: BlobRef) -> bool {
        if !self.ids.insert(blob_ref.blob_id) {
            return false; // duplicate
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
```

### 4.9 Drain-time DB re-check (skip Attached)

**`drain_pending()` does NOT call `mark_attached`.** Pop RAM under lock; **DB re-check outside lock** (avoid holding mutex during RocksDB I/O):

```rust
pub fn drain_pending(&self) -> Vec<BlobRef> {
    let drained = {
        let mut pending = self.pending.lock().expect("lock");
        pending.drain() // pop queue + clear ids under single lock
    };
    drained
        .into_iter()
        .filter(|b| {
            match self.store.is_attached(&b.blob_id) {
                Ok(true) => {
                    debug!(target: "blob", blob_id = ?b.blob_id, "skip drain: already Attached");
                    false
                }
                Ok(false) => true,
                Err(e) => {
                    warn!(target: "blob", blob_id = ?b.blob_id, "drain re-check failed: {e}");
                    true // conservative: allow attach retry
                }
            }
        })
        .collect()
}
```

Filtered-out blobs (already `Attached`) are dropped — **not** returned to the queue.

This closes the crash window where boot re-enqueues a blob whose vertex was sealed but `mark_attached` had not yet run — if mark succeeded before crash, drain skips; if not, attach retries (at-least-once).

In-flight blobs (drained, not yet confirmed) remain `Ready` in RocksDB until `mark_attached` succeeds.

### 4.10 Attach confirmation (`mark_attached`)

Extend `PendingBlobSource` (`crates/consensus/src/ports/pending_blobs.rs`):

```rust
pub trait PendingBlobSource: Send + Sync {
    fn drain(&self) -> Vec<BlobRef>;

    /// Called after blobs were sealed+signed into a local vertex proposal,
    /// before network broadcast. Marks publish state durable (Ready→Attached).
    /// Default: no-op (tests / hosts without custody).
    fn confirm_attached(&self, _blobs: &[BlobRef]) {}
}
```

**Pinned propose ordering** — `vertex_cert::propose_round`:

```rust
// Gate: only after boot_sync_done (§4.7.2)
let blobs = ctx.pending_blobs.drain();  // §4.9: pop RAM, filter is_attached outside lock
let mut vertex = Vertex { ..., blobs: blobs.clone(), ... };
dag::signing::seal_hash(&mut vertex);
let msg = dag::signing::signing_bytes(&vertex);
let proposal = VertexProposal { vertex: vertex.clone(), proposer_sig: ctx.signer.sign_bls(...) };
// seed self partial into VertexBook (in-memory only)
ctx.pending_blobs.confirm_attached(&blobs);  // seal+sign OK → mark_attached (RocksDB)
// push broadcast proposal/partial actions AFTER mark_attached
actions.push(Action::BroadcastVertexProposal(proposal));
// ...
```

Order invariant: **`seal + sign → confirm_attached (mark_attached) → broadcast actions`**. Crash before broadcast loses the wire send but `Attached` in DB prevents re-attach; crash after broadcast but before mark is handled by at-least-once re-propose (duplicate vertex acceptable in P1).

**Dependency rule:** `crates/consensus` depends only on `PendingBlobSource` trait — never on `RocksBlobStore`.

`CustodyPendingBlobs::confirm_attached` → `BlobCustodyHandle::mark_attached(blob_id)` for each blob.

```rust
pub fn mark_attached(&self, blob_id: BlobId) -> Result<()> {
    self.store.mark_attached(&blob_id).map_err(...)
}
```

**`mark_attached` transaction boundary:** single RocksDB update — `PublishRecord.state = Attached` only. No ledger or chunk writes in the same call.

**`confirm_attached` / `mark_attached` failure:** log `warn` + `blob_mark_attached_fail_total++`; do **not** panic. Blob stays `Ready`; boot re-enqueues; drain re-check allows retry. **At-least-once attach** — sealed vertex is not rolled back.

**Crash windows (updated):**

| Crash point | On restart | Outcome |
|---|---|---|
| After drain, before seal | `Ready` in DB | Boot re-enqueue → retry attach |
| After seal, before `mark_attached` | `Ready` in DB | Boot re-enqueue → at-least-once re-propose (duplicate vertex OK) |
| After `mark_attached`, before broadcast | `Attached` in DB | No re-attach; re-broadcast only if needed (P2) |
| After broadcast | `Attached` in DB | Peers may see vertex; local state consistent ✓ |
| `mark_attached` I/O fail after seal | `Ready` in DB | Retry next round; metric++ |

### 4.11 RPC / gossip-failure semantics

**Wire response unchanged** (`Ok(blob_id)` / `Err`).

Document in `submit_blob`:

- If `publish_blob_atomic` succeeds but gossip returns `Err`: blob **is durable** (`Ready`). RPC returns `Err` today.
- **Client MUST NOT infer "blob lost" from gossip failure.**
- P2: distinct success variant or background gossip retry.

### 4.12 End-to-end data flow

```
publish_blob_atomic → enqueue → ledger → gossip
                              │
Boot: scan Ready → validate chunks → rehydrate ledger (light) → re-enqueue
                              │
drain_pending (pop under lock, is_attached filter outside lock)
                              │
propose_round (boot_sync_done): seal + sign → mark_attached → broadcast
```

---

## 5. API changes

### Public (unchanged signatures)

- `lua_submitBlob` RPC — same request/response types.
- `BlobCustodyHandle::publish_payload(&self, payload) -> Result<BlobId>`.
- `BlobCustodyHandle::drain_pending() -> Vec<BlobRef>` — drain + DB re-check; **does not** mark Attached.

### New / changed internal

| Location | Change |
|---|---|
| `BlobCustodyHandle` | `PendingQueue`, `boot_sync_done`, `mark_attached`, drain filter |
| `RocksBlobStore` | `publish_blob_atomic`, `mark_attached`, `is_attached`, `scan_ready_blobs` |
| `PendingBlobSource` | `confirm_attached` default method |
| `vertex_cert::propose_round` | `confirm_attached` after seal+sign |
| `CustodyPendingBlobs` | implement `confirm_attached` |
| `BlobStore` trait | unchanged |

### Metrics (P1)

| Metric | Purpose |
|---|---|
| `blob_publish_atomic_total` | Successful WriteBatch commits |
| `blob_boot_reenqueue_total` | Ready blobs enqueued on boot |
| `blob_ready_count` (gauge) | Ready rows in BlobPublish CF |
| `blob_attached_count` (gauge) | Attached rows in BlobPublish CF |
| `blob_mark_attached_fail_total` | Failed mark_attached calls |
| `blob_orphan_chunk_total` | BlobChunk rows without BlobPublish record |
| `blob_ready_without_chunks_total` | Ready PublishRecord missing expected chunks |
| `blob_boot_duration_ms` (histogram, optional) | Boot recovery latency |

---

## 6. Files touched

| File | Change |
|---|---|
| `crates/storage/src/columns.rs` | Add `BlobPublish` CF |
| `crates/storage/src/stores/blob_publish_store.rs` | **New** |
| `crates/storage/src/stores/blob_chunk_store.rs` | Add `put_batch` |
| `crates/storage/src/db.rs` | Add `scan_cf` |
| `apps/node/src/blob/rocks_store.rs` | Atomic publish + mark/is_attached |
| `apps/node/src/blob/mod.rs` | Flow, boot, PendingQueue, drain filter, ledger rehydrate |
| `apps/node/src/host_context.rs` | `confirm_attached` impl |
| `crates/consensus/src/ports/pending_blobs.rs` | `confirm_attached` trait method |
| `crates/consensus/src/vertex_cert/mod.rs` | Call `confirm_attached` in `propose_round` |
| `apps/node/src/rpc_server.rs` | Doc comment on gossip-failure semantics |
| Tests | See §7 |

---

## 7. Test plan

### Unit tests (`apps/node/src/blob/mod.rs`)

1. **`atomic_publish_all_or_nothing`** — inject failure mid-batch; zero chunks + no PublishRecord.
2. **`publish_then_crash_recovery`** — publish; drop handle; respawn; `drain_pending()` returns blob.
3. **`confirm_attached_no_rebuild`** — publish → drain → confirm → respawn; pending empty; `is_attached` true.
4. **`enqueue_idempotent`** — boot rebuild + manual enqueue same blob_id → one queue entry.
5. **`ledger_rehydrate_lightweight`** — commit atomic; skip ledger on publish; respawn; `is_available` true without reading chunk payloads during boot.
6. **`drain_skips_already_attached`** — mark Attached in DB; blob still in queue → drain returns empty for that blob.
7. **`mark_attached_idempotent`** — Ready→Attached; repeat mark → `Ok(())`, state stays Attached.
8. **`confirm_fail_then_retry`** — inject mark_attached fail; blob stays Ready; second confirm succeeds.
9. **`ready_without_chunks_boot_skip`** — PublishRecord Ready but missing chunks → warn, not enqueued.
10. **`pending_fifo_preserved`** — existing test passes (FIFO best-effort in happy path).

### Consensus tests

1. **`confirm_attached_called_after_propose`** — mock records confirm with correct blobs after seal, before broadcast actions.
2. **`drain_filter_called_before_vertex_build`** — Attached blob not in vertex.blobs.
3. **`propose_blocked_until_boot_sync_done`** — drain returns empty until `boot_sync_done`.
4. **`boot_enqueue_race_mark_attached`** — boot re-enqueues while another thread marks Attached; drain skips blob.

### Storage tests

1. **`blob_publish_store_roundtrip`** — Ready, Attached, Attached→Attached no-op.
2. **`put_batch_atomic_with_wal`** — chunks + publish record one batch.
3. **`writebatch_rollback`** — inject chunk write fail; no PublishRecord persisted.

### Stress / recovery tests

1. **`boot_many_ready`** — 1000 Ready records; boot re-enqueues all; measure boot duration.
2. **`random_crash_recovery`** — publish N blobs; random drop before/after confirm; all eventually Attached or re-enqueued.
3. **`concurrent_publish_same_blob_id`** — two RPC threads; no corrupt PublishRecord (P2 dedup deferred; document behavior).

### Integration

- `blob_custody_smoke`, `submit_blob_rpc`, `l1_distributed_smoke` unchanged behavior.

---

## 8. Migration & rollout

- **New CF:** auto-created on DB open (`create_missing_column_families`).
- **Pre-P1 orphan chunks:** logged + `blob_orphan_chunk_total`; not deleted; not re-enqueued.
- **Wire format:** unchanged. Binary upgrade only.

---

## 9. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Large WriteBatch IO stall | Devnet max ~256 KiB/blob |
| Boot scan at scale | P1 ≪ 10⁴ Ready; lightweight rehydrate; P2 index |
| Gossip fail → RPC Err | Document; P2 gossip worker |
| Duplicate vertex after crash before mark | Drain skip if Attached; at-least-once + idempotent execution |
| `mark_attached` persistent I/O fail | Metric + retry; P2 degraded mode |
| Queue unbounded | P1 assumption: bounded by publish throughput |

---

## 10. Success criteria

- [ ] Failed publish: no partial blob (all shards or none + no Ready record).
- [ ] Restart after publish without drain: blob in next `drain_pending()`.
- [ ] `confirm_attached` success: `Attached`; restart does not re-enqueue.
- [ ] Drain skips blobs already `Attached` in DB.
- [ ] Lightweight ledger rehydrate without chunk payload reads.
- [ ] Idempotent enqueue: no duplicate queue entries per `blob_id`.
- [ ] `propose_round` blocked until `boot_sync_done`.
- [ ] `mark_attached` runs before broadcast actions in propose flow.
- [ ] Gossip ingest unchanged (no pending enqueue).
- [ ] All existing blob + L1 smoke tests green.

---

## 11. Estimated effort

| Item | LOC (est.) |
|---|---|
| Storage (CF + publish store + batch + scan) | ~240 |
| RocksBlobStore + boot rehydrate | ~140 |
| blob/mod.rs PendingQueue + drain filter | ~130 |
| PendingBlobSource + vertex_cert hook | ~40 |
| Tests (expanded §7) | ~480 |
| **Total** | **~1030 LOC, 2 weeks** |
