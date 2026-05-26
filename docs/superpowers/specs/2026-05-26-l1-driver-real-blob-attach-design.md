# Design: L1Driver real blob attach (remove devnet demo blob path)

**Date:** 2026-05-26
**Status:** Draft — design approved, plan pending
**Audience:** Contributors editing `apps/node/src/l1/` and `apps/node/src/blob/`
**Relations:**
- [`2026-05-23-l1-availability-dag-design.md`](2026-05-23-l1-availability-dag-design.md) — phase B/C context
- [`../plans/2026-05-23-07b-l1-blob-custody.md`](../plans/2026-05-23-07b-l1-blob-custody.md) — original 07b plan (Task 7 demo path is what this design replaces)

---

## 1. Problem

`L1Driver` currently has two blob attachment behaviors:

1. **Demo path (devnet-only):** every `demo_blob_every_n_rounds`, the driver fabricates a deterministic random payload via `demo_blob_payload(round, chunk_size)`, calls `BlobCustodyHandle::publish_payload` itself, and attaches a `BlobRef` to *every* certified vertex in that round's quorum.
2. **Real path (none):** payloads submitted through `lua_submitBlob` RPC are stored as chunks and gossiped, but **never referenced** in any vertex header.

The demo path was a smoke-test scaffold from 07b Task 7. It must go: production-shaped behavior is "operator submits payload → driver anchors it to the next vertex." Today the only `BlobRef`s ever appearing in `vertex.blobs` are synthetic.

## 2. Goals

- Delete the demo path entirely (config keys, struct fields, `demo_blob_payload`, devnet.toml entries).
- Connect `lua_submitBlob` submissions to `L1Driver` so submitted blobs appear in vertex headers within one micro-round of submission.
- Preserve current behavior when no blob has been submitted: vertices carry `blobs: vec![]`, cert verify and ingress unaffected.
- Distribute blobs across the 2f+1 vertex authors in a round (round-robin), so a given blob appears in exactly one vertex header — matching the production "per-author proposer" model even while the devnet driver remains centralized.

## 3. Non-goals

- Adding blob sources beyond `lua_submitBlob` RPC (no internal application feed, no gossip-driven re-anchor).
- Backpressure or size limits on the pending queue.
- Persistent mempool: queue is in-memory; node restart drops pending. Submitters must resubmit.
- Per-validator drivers. Centralized devnet driver pattern (one process builds vertices for all 2f+1 authors) is preserved.
- Changes to chunk encoding, custody ledger, erasure config, or wire formats.

## 4. Architecture

### 4.1 Pending-attach queue inside `BlobCustodyHandle`

`BlobCustodyHandle` (apps/node/src/blob/mod.rs) gains one field:

```rust
pending: Arc<Mutex<VecDeque<BlobRef>>>,
```

Initialized empty in `BlobCustody::spawn`. The handle exposes two new methods:

- `pub fn drain_pending(&self) -> Vec<BlobRef>` — pop all queued `BlobRef`s in FIFO order. Called by `L1Driver::tick_round`.
- (private) `fn enqueue_pending(&self, blob: BlobRef)` — push to back. Called at the end of `publish_payload` after all chunks have been stored and gossiped.

The handle continues to be shared (cloned `Arc`-style) between `rpc_server::submit_blob` and `L1Driver`, established in `runtime.rs`. No new wiring through `runtime.rs`.

### 4.2 Data flow

```
client ──RPC──▶ rpc_server::submit_blob
                       │ payload: Vec<u8>
                       ▼
                BlobCustodyHandle::publish_payload
                  1. encode chunks (sequential or erasure)
                  2. for each chunk: publish_tx.send + store.put_chunk + ledger.note_chunk
                  3. build BlobRef { blob_id, commitment, size_bytes }
                  4. enqueue_pending(blob_ref)            ◀── NEW
                  5. return Ok(blob_id)
                       │
                       ▼
                pending: VecDeque<BlobRef>  (FIFO, in-memory)
                       │
                       │ drained each round
                       ▼
                L1Driver::tick_round
                  blobs = custody.drain_pending()
                  batch = build_quorum_vertices_with_blobs(
                            round, valset, parent, real_certs, blobs)
                  for cv in batch: verify, ingest, gossip, emit event
```

**Properties:**

- `enqueue_pending` runs only after every chunk is stored locally. At drain time, `is_available(blob_id)` is already true. No availability check is needed in the driver.
- If `publish_payload` fails mid-loop (e.g. `publish_tx` closed), `enqueue_pending` does not run; the RPC caller receives an error and is responsible for retry. Partial-store chunks remain in Rocks but no `BlobRef` ever enters a vertex header for that attempt.
- Lock contention: one lock per RPC call, one lock per micro-round tick. Devnet cadence is `≥1s`/round; non-issue.

### 4.3 Per-round per-author distribution

`build_quorum_vertices_with_blobs` (apps/node/src/l1/vertex_builder.rs) changes semantics. Today it clones `blobs: Vec<BlobRef>` into all 2f+1 vertices. New behavior:

```rust
pub fn build_quorum_vertices_with_blobs(
    round: u64,
    valset: &ValidatorSet,
    parent_hash: Option<Hash32>,
    real_certs: bool,
    blobs: Vec<BlobRef>,
) -> Vec<CertifiedVertex> {
    let n = u32::try_from(valset.entries.len()).expect("validator count fits u32");
    let quorum = quorum_vertex_count(n);

    let mut buckets: Vec<Vec<BlobRef>> = (0..quorum).map(|_| Vec::new()).collect();
    for (j, b) in blobs.into_iter().enumerate() {
        buckets[j % quorum as usize].push(b);
    }

    (0..quorum)
        .map(|i| {
            let idx = usize::try_from((round + u64::from(i)) % u64::from(n)).expect("index");
            let author = valset.entries[idx].id;
            build_certified_vertex_with_blobs(
                round, author, parent_hash, real_certs, valset,
                std::mem::take(&mut buckets[i as usize]),
            )
        })
        .collect()
}
```

Each blob `j` lands in slot `j % quorum` → exactly one vertex header. Empty queue → all buckets empty → all vertices carry `blobs: vec![]` (matches today's no-demo behavior). When `real_certs = true`, each author's signing root already includes its own `blobs` field, so per-author distinct blob lists remain quorum-verifiable.

### 4.4 `L1Driver` simplification

Driver loses three fields: `demo_blob_enabled`, `demo_blob_every_n_rounds`, `chunk_size`. `blob_custody: Option<BlobCustodyHandle>` stays. The `demo_blobs_for_round` method, `demo_blob_payload` fn, and the `dag::blob::commit::blob_id_from_payload` / `crypto::hash::{blake3_with_dst, dst}` imports are deleted. The new helper:

```rust
async fn pending_blobs_for_round(&self) -> Vec<BlobRef> {
    self.blob_custody
        .as_ref()
        .map(|c| c.drain_pending())
        .unwrap_or_default()
}
```

When `blob_custody = None` (custody disabled), this returns `vec![]` and the driver behaves exactly as before.

## 5. Configuration changes

**Removed from `apps/node/src/config_layers.rs`:**

```rust
pub l1_demo_blob_enabled: bool,
pub demo_blob_every_n_rounds: u64,
fn default_demo_blob_every() -> u64 { 8 }
```

Surviving keys (unchanged): `l1_blob_custody_enabled`, `blob_chunk_size_bytes`, all erasure (07c) keys.

**Removed from `config/profiles/devnet.toml`:**

```toml
l1_demo_blob_enabled = true
demo_blob_every_n_rounds = 8
```

**No new config keys.** The pending queue is fully internal; drain cadence equals micro-round cadence (already configured via `consensus.timing.round_duration_ms`).

## 6. Files affected

| File | Change |
|---|---|
| `apps/node/src/blob/mod.rs` | Add `pending` field, `enqueue_pending` (priv), `drain_pending` (pub); call `enqueue_pending` at end of `publish_payload`; add unit test. |
| `apps/node/src/l1/vertex_builder.rs` | Partition `blobs` by `j % quorum` in `build_quorum_vertices_with_blobs`. |
| `apps/node/src/l1/driver.rs` | Delete demo fields, `demo_blob_payload`, `demo_blobs_for_round`, demo imports. Add `pending_blobs_for_round`. Update existing test signature; add round-robin attach test. |
| `apps/node/src/runtime.rs` | `L1Driver::new` call site loses 3 demo args. |
| `apps/node/src/config_layers.rs` | Remove 2 fields + `default_demo_blob_every`. |
| `config/profiles/devnet.toml` | Remove 2 demo lines. |

No new files. No new modules.

## 7. Test plan

### Unit (apps/node/src/blob/mod.rs)

`pending_queue_fifo_and_drain`:
1. Spawn `BlobCustody` with an `mpsc::Sender` for `publish_tx` whose receiver is drained by a no-op task.
2. Call `handle.publish_payload(payload_a)`, `..._b`, `..._c` sequentially.
3. Assert `handle.drain_pending()` returns three `BlobRef`s with `blob_id` matching each payload, in order `[a, b, c]`.
4. Assert a second `drain_pending()` returns `vec![]`.

### Unit (apps/node/src/l1/driver.rs)

`tick_emits_quorum_events_per_round` (existing): update `L1Driver::new` call to drop 3 demo args; assertions on quorum size and event count unchanged.

`pending_blobs_attached_round_robin` (new):
1. Build `L1Driver` with `real_certs = true` and a real `BlobCustodyHandle` (sequential mode, small `chunk_size` e.g. 1024).
2. Submit 5 distinct payloads through `handle.publish_payload`.
3. Invoke `driver.tick_round()` once.
4. Assert: quorum = 3 for devnet-4; collected `BlobRef`s across the 3 vertices total 5; per-vertex counts are `[2, 2, 1]`; every emitted `BlobRef` is unique (each blob appears in exactly one header); `handle.drain_pending()` afterwards returns `vec![]`; every vertex passes `dag::cert::verify_certified_vertex`.

### Integration / smoke

- Existing `blob_custody_smoke.rs` flow (RPC submit → custody available) must still pass.
- Add follow-up assertion that one driver tick after submit yields a vertex whose `blobs` contains the submitted `blob_id`.

### Acceptance

- `cargo test` on the workspace passes.
- `rg 'demo_blob|l1_demo_blob_enabled|demo_blob_every'` returns zero hits in `apps/`, `crates/`, `config/`.
- Devnet 4-node compose: submit one blob via `lua_submitBlob` → the next round's certified vertex set contains that `blob_id` in exactly one vertex header → all four nodes report `lua_blobStatus = available` for that `blob_id`.
- Macro finality unaffected (existing devnet E2E still reaches finality).

## 8. Backwards compatibility & migration

- **Breaking config:** any `local.toml` or override profile setting `l1_demo_blob_enabled` or `demo_blob_every_n_rounds` may fail to parse depending on serde strictness of `NodeSection`. The implementation must verify whether `ProfileFile` rejects unknown keys; if it does, the release note must call out the removal. If serde silently ignores unknown keys, no operator action is required, but the release note still records the removal so operators can clean up their configs.
- **Mempool restart loss:** documented above (§3 non-goal). Submitters retry on restart.
- **Wire compatibility:** unchanged. `BlobRef`, `Vertex`, `CertifiedVertex` encodings are not touched. Old peers and new peers interop the same way; new peers simply emit non-empty `blobs` when an operator submits.

## 9. Risks & open questions

- **Single-caller assumption.** `enqueue_pending` is invoked inside `publish_payload`. If a future change adds a second caller to `publish_payload` (e.g. an internal feed), every payload it submits will also flow into vertex headers — exactly what this design wants for "RPC-only" today, but a quiet broadening of contract later. Mitigation: explicit doc comment on `publish_payload` describing the enqueue side effect.
- **Unbounded queue.** With no rate limit on RPC submissions, a hostile (or buggy) operator can grow `pending` arbitrarily. Devnet trust model accepts this; testnet/mainnet will need a bound. Tracked for follow-up.
- **Centralized driver assumption baked into round-robin.** The `j % quorum` partition is well-defined only because one process owns the queue and builds all 2f+1 vertices. Per-validator drivers (a future change) will need a different partition policy — each validator builds only its own author's vertex and attaches its own local pending queue. This design does not preclude that change.
