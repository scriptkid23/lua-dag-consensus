# Design: L1 Availability DAG + Real Vertex Certificates

**Date:** 2026-05-23  
**Status:** Draft (plan-ready)  
**Audience:** Contributors extending L1 beyond 06b-L1 driver/ingress  
**Prerequisite:** 06b-L1 landed (`LiveDag`, `L1Driver`, gossip `certified-vertex`, devnet E2E)  
**Relations:** [`2026-05-11-folder-architecture-design.md`](2026-05-11-folder-architecture-design.md) §L1; [`2026-05-23-06b-l1-vertex-driver.md`](../plans/2026-05-23-06b-l1-vertex-driver.md)

---

## 1. Problem

Today L1 is a **thin stub**:

| Area | Today | Gap |
|------|-------|-----|
| Vertex certificate | Fixed `BlsSig([0xAB; 96])`, bitmap `0xFF` | No BLS sign/verify; any peer can inject arbitrary vertices |
| Vertex hash | `SIM_VERTEX_HASH(round ‖ author)` only | Not content-addressed over parents/blobs |
| Blobs | Always `blobs: []` | No payload, custody, or availability |
| Crate layout | Logic split across `apps/node/src/l1/` and `apps/sim/` | No shared `crates/dag/` per folder-architecture |
| Ingress | `LiveDag::ingest` accepts any `CertifiedVertex` | No cryptographic gate before Bullshark |

Bullshark intentionally does **not** verify L1 certs today; verification belongs to the **host/L1 layer** before vertices enter `DagView`.

---

## 2. Goals (phased)

Land L1 in **three independent vertical slices**, each shippable and testable:

| Phase | ID | Delivers |
|-------|-----|----------|
| **A** | **07a-l1-vertex-cert** | Real BLS quorum certificates on `CertifiedVertex`; verify on ingress; shared `crates/dag/` cert module |
| **B** | **07b-l1-blob-custody** | Blob payload store, custody ack, wire topic for chunks; vertices may carry `BlobRef`s |
| **C** | **07c-l1-erasure-da** | Erasure-coded chunks, sampling/challenges skeleton, causal-set RPC handler |

**Non-goals (all phases):**

- L4 Bitcoin anchor
- Full DAS / 2D RS / KZG (folder-arch §10 defers to `crates/dag/das/`)
- Replacing Narwhal/Tendermint-style separate mempool workers (devnet keeps host-side `L1Driver` tick producer)
- Changing Bullshark commit rules or SM `Action` enum for L1 publish

---

## 3. Architectural decision: `crates/dag/` owns L1 semantics

```
types::dag::{Vertex, CertifiedVertex, BlobRef}   ← wire shapes (unchanged)
crates/dag/                                     ← NEW: L1 algorithms + verify
  signing.rs    content hash + signing root
  cert.rs       quorum sign / aggregate / verify
  blob/         (phase B) custody store trait + Rocks impl hook
  erasure/      (phase C) chunk grid + encode/decode stubs
consensus::ports::DagView                       ← unchanged trait
apps/node::LiveDag                              ← storage + index only; no crypto
apps/node::orchestrator                         ← calls dag::cert::verify before ingest
apps/node::l1::L1Driver                         ← calls dag::cert to build real certs
apps/sim::vertex_factory                        ← delegates to dag::cert in phase A
```

**Rejected:** putting verify inside `LiveDag::ingest` — mixes persistence with crypto policy; orchestrator + gossip ingress are the policy boundary.

**Rejected:** new SM `Action::BroadcastCertifiedVertex` — L1 feed stays host-side (06b-L1 lock-in).

---

## 4. Phase A — Real vertex BLS certificates (07a)

### 4.1 Certificate model

Narwhal-class **quorum certificate** over a single vertex header:

1. **Signing root** — canonical bytes of `(round, author, parents, blobs)` **without** the `hash` field.
2. **Content hash** — `blake3_with_dst(VERTEX_HASH, signing_root_bytes)` stored in `vertex.hash`.
3. **Per-validator signature** — `sign(sk_i, VERTEX_CERT, signing_root)`.
4. **Quorum certificate** — aggregate `2f+1` distinct validator signatures into `BlsAggSig { sig, bitmap }` where bitmap indices match valset order.

Verification (ingress):

1. Recompute content hash; reject if `vertex.hash` mismatch.
2. Collect pubkeys for set bits in `certificate.bitmap`; require count ≥ `2f+1`.
3. `verify_aggregate(pks, VERTEX_CERT, signing_root, &certificate.sig)`.

### 4.2 New DSTs (append-only)

| DST | Purpose |
|-----|---------|
| `lua-dag/v1/vertex-hash` | Content hash of vertex body |
| `lua-dag/v1/vertex-cert` | BLS aggregate domain for quorum cert |

`SIM_VERTEX_HASH` remains for legacy sim tests behind a config flag; devnet profile switches to production hash.

### 4.3 Devnet quorum production

Devnet knows all validator keys (`devnet_keys` / valset TOML). Phase A **does not** require a separate certificate protocol round on the wire:

- `L1Driver` / `vertex_builder` builds `2f+1` vertices per tick (unchanged cadence).
- For each vertex, `dag::cert::build_devnet_quorum_cert(&vertex, &valset, quorum_indices)` signs with each author's devnet BLS IKM and aggregates.
- Each live node still only **locally signs vertices it authors** in a follow-up (07a+); phase A may sign all quorum members centrally on every node for devnet parity with sim (same as macro fixture multi-signer pattern in tests).

**Config gate:** `[node].l1_real_vertex_certs = true` (devnet); `false` keeps fixture `[0xAB]` for regression tests.

### 4.4 Ingress gate

Both paths call the same verifier:

- **Gossip:** `gossip_wire::decode_certified_vertex` → verify → `Event::CertifiedVertexReceived`
- **Local driver:** verify before `LiveDag::ingest` and before `events_tx.send`

Invalid vertices: log + drop (no SM step). Metric: `vertex_cert_rejected_total`.

### 4.5 Acceptance (phase A)

- Unit: sign → aggregate → verify roundtrip in `crates/dag/tests/cert_roundtrip.rs`
- Unit: wrong hash / wrong bitmap / insufficient signers → reject
- Node integration: `l1_real_vertex_certs = true` → existing `l1_driver_smoke` + `l1_gossip_roundtrip` pass
- Negative: tampered gossip payload rejected before DAG mutation
- Devnet E2E (`devnet_e2e_smoke.sh`) still reaches macro finality

---

## 5. Phase B — Blob custody (07b)

### 5.1 Scope

- `crates/dag/blob/store.rs` — trait `BlobStore { put_chunk, get_chunk, has_blob }`
- Rocks column family `blob_chunk` (key: `blob_id ‖ chunk_index`)
- Gossip topic `blob-chunk` (Borsh payload)
- Vertices may include `BlobRef { blob_id, commitment, size_bytes }`; commitment phase B = `blake3_with_dst(BLOB_COMMIT, payload)` (KZG deferred to 07c)
- Custody rule: vertex cert verifies **header** only; blob availability checked when chunk count ≥ threshold (simple `size_bytes` + chunk count match for phase B)

### 5.2 Non-goals (phase B)

- Erasure coding (phase C)
- DA slashing evidence emission
- Cross-validator blob reconciliation

---

## 6. Phase C — Erasure + DA skeleton (07c)

- `crates/dag/erasure/` — RS encode/decode over blob bytes (fixed `k`, `n` from config)
- `ChunkRef` populated in store
- `crates/net/src/rpc/causal_set.rs` — minimal handler: return certified vertex hashes for round range
- Challenge struct + verify hook (no on-chain slash yet)

---

## 7. Plan decomposition

| Plan file | Depends on |
|-----------|------------|
| [`2026-05-23-07a-l1-vertex-cert.md`](../plans/2026-05-23-07a-l1-vertex-cert.md) | 06b-L1 |
| `2026-05-23-07b-l1-blob-custody.md` (future) | 07a |
| `2026-05-23-07c-l1-erasure-da.md` (future) | 07b |

---

## 8. Risks

| Risk | Mitigation |
|------|------------|
| Wire break (hash recipe change) | Config flag; wipe devnet Rocks dirs (`docker compose down -v`) |
| Bitmap index vs valset order drift | Single helper `valset_signer_indices(set, authors)` used by sign and verify |
| Duplicate verify (gossip + orchestrator) | Idempotent: orchestrator verifies; gossip path verifies before enqueue (driver skips re-verify on self-produced) |
| Scope creep into full Narwhal | Explicit non-goals per phase; host-side driver unchanged |

---

## 9. Self-review

- [x] Placeholder scan: phases B/C scoped but not vague — concrete deliverables listed
- [x] Consistency: verify at host boundary, not in Bullshark/SM
- [x] Decomposition: three plans, each independently testable
- [x] Ambiguity: signing root excludes `hash` field — locked
