# Design: L1 Data Plane — verified ingest, unified availability, repair plane

**Date:** 2026-07-19  
**Status:** Draft — awaiting review  
**Audience:** Contributors editing `apps/node/src/blob/`, `crates/dag/src/blob/`, `crates/dag/src/da/`, `crates/net/`, `crates/storage/`  
**Relations:**
- [`docs/architecture/layer-1.md`](../../architecture/layer-1.md) — Data Plane diagram
- [`2026-05-23-l1-availability-dag-design.md`](2026-05-23-l1-availability-dag-design.md) — Availability DAG framing
- [`2026-07-06-erasure-only-blob-path-design.md`](2026-07-06-erasure-only-blob-path-design.md) — RS 4/8 only path
- [`2026-07-13-blob-atomic-publish-p1-design.md`](2026-07-13-blob-atomic-publish-p1-design.md) — atomic publish + crash-safe pending (landed)
- Multi-model roundtable (2026-07-19): ChatGPT · Gemini · Kimi · DeepSeek — diagnose → critique → converge

**Changelog:**
- **Rev 1:** Initial draft from code review + 3-round architecture brainstorm.

---

## 1. Problem

Phase P1 (atomic publish) made local durability correct: all `n` shards + `PublishState::Ready` land in one WAL batch, and Ready blobs re-enqueue after restart. The Data Plane still has three structural gaps that block production-shaped availability:

### G1 — Multiple ingest paths, no shared verification gate

Shards enter storage through three code paths with different rules:

| Path | Persist | Ledger update | RS-Merkle verify |
|---|---|---|---|
| `publish_payload` (local) | `publish_blob_atomic` | `note_chunk` + decode | Commitment computed locally; no per-shard proof check |
| Gossip ingest (`BlobCustody::run`) | `put_chunk` | `note_chunk` + decode | **None** before put |
| Boot rehydrate | already on disk | `note_chunk_present` (count only) | **None** |

`AvailabilityChallenge` / `AvailabilityResponse` + `verify_availability_response` exist in `crates/dag/src/da/challenge.rs` but are unwired skeletons. Invalid or adversarial gossip chunks can pollute `BlobChunk` CF.

### G2 — Availability semantics are path-dependent

| Context | Rule |
|---|---|
| Runtime (`erasure_available`) | ≥ `k` shards present **and** `decode_shards` succeeds |
| Boot (`validate_chunks`) | **all `n`** shards must exist |
| Boot ledger (`note_chunk_present`) | ≥ `k` by **count only** (no decode) |

A blob that was locally available before crash can fail boot re-enqueue despite still satisfying the RS threshold. Operators and RPC consumers (`lua_getBlobStatus` / `locally_available`) see inconsistent meaning of "available".

### G3 — Push-only dissemination with no repair

- `publish_payload` enqueues pending **before** gossip; gossip send failure is warn-only.
- There is no `ChunkFetch` / IWANT / anti-entropy path.
- Publisher floods all `n` shards on Gossipsub (acceptable for current N; not a Phase 0 blocker, but repair is).

### Hot-path debt (not blocking correctness, tracked)

| ID | Issue |
|---|---|
| F1 | Double RS encode: `encode_shards` then `blob_ref_commitment` encodes again |
| F4 | `erasure_available` re-reads RocksDB + full decode on every transition attempt |
| F5 | `list_chunk_refs` hardcodes scan `0..64` |
| F6 | `erasure_chunks` clones every shard; ledger `Mutex` taken twice per chunk |
| F9 | No TTL/prune for Attached `blob_chunk` rows |

---

## 2. Goals

1. **Single verified ingest gate:** every accepted shard (local publish, gossip, repair, boot replay) passes one API before RocksDB mutation and before availability transitions.
2. **Unified local availability state machine:** one lifecycle and one threshold rule (≥ `k` verified shards + recoverability check) for runtime and boot.
3. **CustodyLedger stays local-only:** `is_available` / `locally_available` never claim network-wide DA. Network DA remains Control Plane / certification responsibility.
4. **Repair plane (Phase 1):** incomplete blobs converge via pull (`ChunkFetch`) without republishing the full payload; ChunkFetch feeds the same ingest gate.
5. **Preserve P1 invariants:** atomic publish batch; PendingQueue as RAM cache of Ready records; only local publish (and boot re-enqueue of Ready) feeds `enqueue_pending`.
6. **Architecture doc honesty:** `docs/architecture/layer-1.md` matches code contracts (enqueue vs gossip ordering; local vs quorum availability).

---

## 3. Non-goals (next 90 days)

| Non-goal | Why |
|---|---|
| Replace RS-Merkle with KZG (or any commitment migration) | Out of scope; RS-Merkle is locked |
| Quorum / replicated CustodyLedger | Violates Narwhal Data/Control split |
| DHT / IPFS-style chunk discovery | Closed validator set; direct peer pull is enough |
| Full DA slash emission from challenges | Wire + verify first; slash policy later |
| Publisher sampling / announce-only dissemination as Phase 0 | Keep flood-all until repair works; redesign fanout in Phase 2 |
| Exactly-once vertex attach / lease-ack | Unchanged from P1 |
| Changing RS parameters (`k`/`n`/shard size) or max blob 128 KiB | Separate change if needed |

---

## 4. Decisions (locked from Round 3)

| ID | Decision |
|---|---|
| D1 | **ChunkFetch is transport only.** It never writes RocksDB directly. All accepted shards enter through the common ingest API. The availability state machine owns the sole transition into `LocallyAvailable`. |
| D2 | **Boot threshold = ≥ `k` verified shards**, same as runtime. Prefer **lazy decode** at boot (verify recoverability without reconstructing payload unless demanded). Minority dissent (keep all `n` at cold boot) is rejected for Phase 0. |
| D3 | **Phase 0 keeps push-all-n gossip.** Phase 1 adds BlobRef advertisement + ChunkFetch. Phase 2 may move to announce+pull or sample `k+ε`. |
| D4 | **Batched `RepairWindow`**, not greedy per-chunk. Priority ≈ `age × missing_shard_count` (+ exponential backoff). |
| D5 | **Build order:** verified ingest + state machine + Merkle gate → repair plane → lazy-decode / dissemination economics / prune. |

---

## 5. Architecture

### 5.1 Source of truth (unchanged roles)

```
RocksDB BlobChunk CF          durable shard bytes
RocksDB BlobPublish CF        PublishRecord Ready | Attached  (pending source of truth)
CustodyLedger (RAM)           derived local completeness index
PendingQueue (RAM)            derived Ready attach queue for AuthorLoop
Availability SM (RAM + optional marker)  derived local lifecycle
```

`CustodyLedger` remains **derived**. Do not persist a second mutable custody database. Optional Phase 0/1: a small durable **availability marker** (blob_id → state) may be written only as a cache of a successful verify, never as independent truth.

### 5.2 Single ingest entry point

Introduce a module-level API (names illustrative; keep in `apps/node/src/blob/`):

```rust
pub enum ChunkSource {
    LocalPublish,
    Gossip,
    ChunkFetch,   // Phase 1; reserved in Phase 0 enum
    BootReplay,
}

pub struct IngestOutcome {
    pub blob_id: BlobId,
    pub newly_available: bool,
    pub state: LocalAvailability,
}

/// Sole legal path to persist a verified shard and advance local availability.
pub fn ingest_verified_chunk(
    ctx: &BlobIngestCtx,
    source: ChunkSource,
    chunk: &BlobChunk,
    // Phase 0: for LocalPublish, proof may be implicit (publisher computed root).
    // For Gossip/Fetch: require merkle proof against known BlobRef.commitment when available.
    proof: Option<&ShardMerkleProof>,
) -> Result<IngestOutcome>;
```

**Rules:**

1. No other code path may call `put_chunk` / `publish_blob_atomic` chunk inserts for inbound foreign shards without going through this function (local atomic publish may batch-write shards then call ingest for ledger transitions — see §5.4).
2. Reject chunk if shard length ≠ `data_shard_size`, index ≥ `n`, or merkle proof fails when a commitment is known.
3. Idempotent: re-ingest of an already-stored `(blob_id, index)` is a no-op success (metrics: `blob_chunk_duplicate_total`).
4. On first transition to `LocallyAvailable`, increment existing `blob_available` metric.

### 5.3 Local availability state machine

```
Unknown
   │ register_erasure / first chunk meta
   ▼
Partial          (≥1 shard, <k verified OR not yet recoverability-checked)
   │ ≥k verified shards
   ▼
Recoverable      (recoverability check passed — see below)
   │ synonym for RPC / is_available
   ▼
LocallyAvailable
```

**Recoverability check (unified):**

- Prefer: verify shard inclusion against `BlobRef.commitment` (RS-Merkle) for ≥ `k` shards, then optionally run `decode_shards` once.
- Phase 0 minimum: if commitment known, verify ≥ `k` leaves against root; if decode is used, run it **at most once** per blob until success (cache result in RAM / optional marker CF). Do **not** re-decode on every subsequent `note_chunk`.
- Boot uses the **same** rule. Delete the "all `n` required" gate in `validate_chunks` for Ready re-enqueue; require ≥ `k` present + recoverability check instead.
- Remove trust in `note_chunk_present` count-only → Available. Boot must call the same recoverability path (or mark Partial until check runs).

**RPC contract:** keep field name `locally_available` (or document alias). Never rename to imply quorum DA without a separate field.

### 5.4 Publish path (Phase 0)

Target flow:

```
encode_shards once
  → commitment = rs_merkle_commitment(&shards)   // no second encode (fix F1)
  → publish_blob_atomic(chunks, Ready record)
  → for each chunk: ingest_verified_chunk(LocalPublish, …)
  → enqueue_pending(BlobRef)                     // still after durable commit
  → gossip each chunk (best-effort; warn on fail remains Phase 0)
```

**Ordering vs architecture doc:** Phase 0 **keeps** durable-store → enqueue → gossip (crash-safe attach). Update `docs/architecture/layer-1.md` to match: enqueue after atomic store, not after gossip success. Gossip reliability is Phase 1 repair's job, not a publish blocker.

### 5.5 Gossip ingest path (Phase 0)

```
recv BlobChunk
  → resolve commitment if known (from prior BlobRef / publish record / future advert)
  → ingest_verified_chunk(Gossip, chunk, proof?)
  → on reject: metric + drop (no store)
```

If commitment is **unknown** at first sight of a gossip chunk:

- **Phase 0 policy:** store only after basic structural checks (index/length/`n_shards` match local `ErasureConfig`); mark ledger Partial; do **not** mark LocallyAvailable until a commitment is known and recoverability passes. Prefer rejecting unknown-commitment chunks if that breaks too many tests — document the chosen policy in the implementation plan and add a metric `blob_chunk_unknown_commitment_total`.

Recommended Phase 0 default: **reject gossip chunks whose `blob_id` has no known commitment** unless the node already has a `PublishRecord` or registered meta for that blob. This prevents unbounded disk pollution.

### 5.6 Merkle proof on the wire (Phase 0 scoped)

Today `BlobChunk` carries raw shard bytes without an inclusion proof. Options:

| Option | Phase | Notes |
|---|---|---|
| A. Extend `ChunkPayload::Erasure` with `proof: Vec<Hash32>` (or compact path) | Prefer Phase 0 for Gossip | Wire break; pre-production OK if all nodes upgrade together (same precedent as sequential removal) |
| B. Verify only after ≥ `k` via full decode + re-encode commitment check | Phase 0 fallback | Matches existing `verify_availability_response` pattern; heavier CPU; no wire change |

**Decision for this design:** Phase 0 implements **Option B** for gossip (decode/recommitment when ≥ `k`, cache result) and lands **Option A** wire proofs as soon as repair (Phase 1) needs cheap per-shard verify — can ship in the same PR train as ChunkFetch. Implementation plan may split A into Phase 0.1 if wire churn is cheap.

Either way: never mark LocallyAvailable without a commitment match.

### 5.7 Repair plane (Phase 1)

```
BlobRef advertisement (gossip topic, e.g. lua-dag/v1/blob-ref)
  → peers learn commitment + size + n/k
Incomplete local blob (Partial, aged past RepairWindow)
  → ChunkFetch(request missing indices) to peers that advertised / mesh peers
  → responses feed ingest_verified_chunk(ChunkFetch, …)
```

- Reuse / wire `AvailabilityChallenge` + `AvailabilityResponse` shapes where they fit; extend if request/response needs peer addressing.
- Scheduler: every `RepairWindow` (config, default 200–500 ms), select up to `max_repairs` blobs by priority `age_ms * missing_count`, with per-blob backoff.
- Rate-limit inbound/outbound fetch to avoid amplification.
- Success criterion: after artificial packet loss in integration tests, nodes reach LocallyAvailable without republish.

### 5.8 Dissemination economics (Phase 2)

- Move from flood-all-n toward BlobRef announce + pull, or publisher sample `k+ε`.
- Add TTL/prune for Attached chunks after retention window.
- Lazy decode / `ChunkIngestCache` for hot-path CPU (F4).
- Fix F5/F6 hygiene.

---

## 6. Module ownership

| Concern | Owner |
|---|---|
| `ingest_verified_chunk`, publish/gossip wiring | `apps/node/src/blob/` |
| `CustodyLedger` + recoverability helpers | `crates/dag/src/blob/custody.rs` |
| RS encode/decode, merkle commit/proof | `crates/dag/src/erasure/` |
| Challenge verify hooks | `crates/dag/src/da/challenge.rs` |
| Gossip topics + ChunkFetch framing | `crates/net/` |
| Chunk / publish CFs | `crates/storage/` |
| Architecture diagram sync | `docs/architecture/layer-1.md` |

---

## 7. Phased delivery

### Phase 0 — Verified ingest + unified availability (Effort: M)

**Goal:** one correctness path; boot == runtime threshold.

**Deliverables:**

- `ChunkSource` + `ingest_verified_chunk` (or equivalent name)
- Refactor gossip `run()` and publish ledger updates through it
- Unified state machine; ≥ `k` boot re-enqueue; kill count-only Available
- Fix F1 double encode
- Decode/verify at most once per blob until Available (address F4 partially)
- Update `layer-1.md` enqueue/gossip wording + local-availability callout
- Metrics: duplicate, reject, unknown-commitment, recoverability-fail

**Exit criteria:**

- Unit/integration: invalid shard never becomes LocallyAvailable
- Crash mid-publish still recovers Ready via P1 path
- Boot with exactly `k` valid shards re-enqueues; with `k-1` does not mark Available
- Existing blob custody / erasure recovery tests updated and green

### Phase 1 — Repair plane (Effort: L)

**Goal:** eventual local availability under loss.

**Deliverables:**

- BlobRef advertisement
- ChunkFetch req/resp (+ rate limits)
- RepairWindow scheduler + metrics
- Wire challenge verify on fetch responses
- Optional per-shard merkle proofs on wire (Option A)

**Exit criteria:**

- Multi-node test: drop gossip for some shards → fetch fills gaps → LocallyAvailable
- No direct store writes from fetch path (asserted by code structure / review)

### Phase 2 — Scale + ops (Effort: M)

**Goal:** bandwidth, boot latency, retention.

**Deliverables:** announce+pull or sample `k+ε`; lazy decode cache; TTL prune; `list_chunk_refs` uses `n`; lock hygiene.

**Exit criteria:** bandwidth/blob no longer O(n × mesh) flood; boot time not dominated by full payload decode of all Ready blobs.

---

## 8. Testing plan (Phase 0 focus)

| Test | Asserts |
|---|---|
| `ingest_rejects_bad_shard_length` | Structural reject |
| `ingest_idempotent_duplicate` | Second put no-ops |
| `gossip_cannot_mark_available_without_commitment` | Policy in §5.5 |
| `recoverability_once_then_cache` | No N decodes for N arrivals after Available |
| `boot_reenqueues_with_k_shards` | Replaces all-`n` requirement |
| `boot_does_not_count_only_available` | `note_chunk_present` alone insufficient |
| `publish_single_encode_commitment` | F1 regression |
| Existing: `blob_custody_smoke`, `erasure_recovery`, `blob_gossip_roundtrip`, atomic publish tests | Still pass |

Phase 1 adds: loss-injection gossip + fetch convergence; fetch rate-limit abuse case.

---

## 9. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Wire break for merkle proofs | Pre-production coordinated upgrade; or ship Option B first |
| Rejecting unknown-commitment gossip reduces opportunistic custody | BlobRef advert in Phase 1; until then publishers still flood full shards |
| Boot decode CPU if eager | Spec chooses lazy; only verify when promoting Ready / Available |
| Repair amplification | Caps, backoff, RepairWindow batching |
| Doc/code drift | Explicit §5.4 doc update in Phase 0 acceptance |

---

## 10. Open points for implementation plan (not blockers for this design)

1. Exact Rust names/module split (`BlobIngest` vs methods on `BlobCustodyHandle`).
2. Whether availability marker gets its own CF in Phase 0 or stays RAM-only until Phase 1.
3. Default `RepairWindow` / `max_repairs` numeric values (Phase 1).
4. Topic string for BlobRef advertisement (Phase 1).

---

## 11. Recommendation to engineering lead

Ship **Phase 0** first: lock `ingest_verified_chunk` + unified ≥ `k` availability before any dissemination redesign. Build ChunkFetch on that gate in Phase 1. Treat announce+pull / sampling / prune as Phase 2.

---

## 12. Approval

Please review this spec and note requested changes. After approval, the next step is an implementation plan under `docs/superpowers/plans/` for **Phase 0 only** (Phase 1/2 as follow-up plans).
