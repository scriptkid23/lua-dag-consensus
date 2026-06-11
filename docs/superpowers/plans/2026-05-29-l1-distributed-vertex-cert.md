# L1 Distributed Vertex Certification — Implementation Plan

> **SUPERSEDED (2026-06-11):** This plan implements the 2026-05-29 design, which was replaced by
> [`2026-06-04-distributed-vertex-certificate-design.md`](../specs/2026-06-04-distributed-vertex-certificate-design.md)
> (vertex_cert module in `crates/consensus`, proposer-only aggregation, cert-driven round advancement).
> Do **not** execute this plan. Current plan: [`2026-06-11-distributed-vertex-certificate.md`](2026-06-11-distributed-vertex-certificate.md).

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace centralized `L1Driver` with per-validator `AuthorLoop` + `CertCollector` so L1 produce matches the approved production path: proposal → partials → ≥ `2f+1` QC → verified `CertifiedVertex` → `LiveDag`.

**Architecture:** New gossip topics `vertex-proposal` and `vertex-partial` feed an in-memory `CertCollector` on every node; any node aggregates partials and publishes `certified-vertex`. Each validator's `AuthorLoop` ticks on `round_duration_ms`, gates on ≥ `2f+1` CVs at `r-1`, builds one `Vertex` (`author = self`, multi-parent), signs via `SignerPort`, and publishes proposal+partial. Blob `drain_pending` moves from `L1Driver` to `AuthorLoop` only. Delete `apps/node/src/l1/driver.rs` and remove `l1_driver_enabled` produce path.

**Tech Stack:** Rust 1.88, tokio, libp2p gossipsub, `borsh`, `dag::cert` / `dag::signing`, existing `DevSigner` + valset TOML.

**Spec:** [`docs/superpowers/specs/2026-05-29-l1-distributed-vertex-cert-design.md`](../specs/2026-05-29-l1-distributed-vertex-cert-design.md) (Approved locked)

---

## File map

| File | Action |
|------|--------|
| `crates/types/src/dag/vertex_partial.rs` | **CREATE** `VertexPartial` wire struct |
| `crates/types/src/dag/mod.rs` | export `VertexPartial` |
| `crates/dag/src/cert.rs` | **MODIFY** `build_cert_from_partials`, stricter `verify_certified_vertex`, `pub quorum_threshold` |
| `crates/dag/tests/cert_from_partials.rs` | **CREATE** unit tests |
| `crates/net/src/gossip/topics.rs` | **MODIFY** `VertexProposal`, `VertexPartial` topics |
| `crates/net/src/gossip_wire.rs` | **MODIFY** encode/decode + swarm ingress routing |
| `crates/net/src/swarm_runner.rs` | **MODIFY** subscribe + fan-in channels |
| `crates/net/tests/vertex_gossip_roundtrip.rs` | **CREATE** proposal/partial roundtrip |
| `apps/node/src/l1/propose.rs` | **CREATE** `may_propose_round`, `parents_for_round`, `next_propose_round` |
| `apps/node/src/l1/cert_collector.rs` | **CREATE** partial map + aggregate + publish CV |
| `apps/node/src/l1/author_loop.rs` | **CREATE** tick loop |
| `apps/node/src/l1/mod.rs` | export new modules; drop `driver` |
| `apps/node/src/l1/driver.rs` | **DELETE** |
| `apps/node/src/l1/vertex_builder.rs` | **MODIFY** keep `build_certified_vertex_with_blobs` for tests only OR move helpers to `dag`; remove quorum batch from node runtime |
| `apps/node/src/l1/parent.rs` | **DELETE** or keep only if sim still needs — node uses `propose.rs` |
| `apps/node/src/live_dag.rs` | **MODIFY** `certified_count_at_round`, `max_certified_round` |
| `apps/node/src/runtime.rs` | spawn `AuthorLoop` + `CertCollector`; remove `L1Driver` |
| `apps/node/src/config_layers.rs` | `l1_author_loop_enabled` replaces `l1_driver_enabled` |
| `config/profiles/devnet.toml` | config rename |
| `apps/node/tests/l1_author_loop_smoke.rs` | **CREATE** replaces `l1_driver_smoke` |
| `apps/node/tests/l1_vertex_partial_roundtrip.rs` | **CREATE** 2-node partial → CV |
| `apps/node/tests/l1_driver_smoke.rs` | **DELETE** or rewrite |
| `apps/node/tests/l1_gossip_roundtrip.rs` | **MODIFY** for distributed path |
| `docs/superpowers/specs/2026-05-29-l1-distributed-vertex-cert-design.md` | status → plan-ready |

**Invariant:** `apps/node` runtime MUST NOT call `dag::cert::build_quorum_cert` with `devnet_bls_ikm` for indices ≠ self.

---

## Task 1: `VertexPartial` type

**Files:**
- Create: `crates/types/src/dag/vertex_partial.rs`
- Modify: `crates/types/src/dag/mod.rs`

- [ ] **Step 1: Add type**

```rust
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct VertexPartial {
    pub vertex_hash: Hash32,
    pub validator: ValidatorId,
    pub sig: BlsSig,
}
```

- [ ] **Step 2: Export from `types::dag`**

- [ ] **Step 3: Run**

`cargo check -p types --locked`

- [ ] **Step 4: Commit**

```bash
git add crates/types/src/dag/
git commit -m "feat(types): add VertexPartial for L1 distributed QC wire"
```

---

## Task 2: Gossip topics + encode/decode

**Files:**
- Modify: `crates/net/src/gossip/topics.rs`
- Modify: `crates/net/src/gossip_wire.rs`
- Create: `crates/net/tests/vertex_gossip_roundtrip.rs`

- [ ] **Step 1: Add wire constants**

```rust
pub const VERTEX_PROPOSAL: &str = "lua-dag/v1/vertex-proposal";
pub const VERTEX_PARTIAL: &str = "lua-dag/v1/vertex-partial";
```

Add `Topic::VertexProposal`, `Topic::VertexPartial` variants; include in `subscribe_set`.

- [ ] **Step 2: Encode/decode helpers**

```rust
pub fn encode_vertex_proposal(v: &Vertex) -> Result<(Topic, Vec<u8>)>;
pub fn encode_vertex_partial(p: &VertexPartial) -> Result<(Topic, Vec<u8>)>;
pub fn decode_vertex_proposal(topic: &str, data: &[u8]) -> Result<Option<Vertex>>;
pub fn decode_vertex_partial(topic: &str, data: &[u8]) -> Result<Option<VertexPartial>>;
```

`decode_*` returns `Ok(None)` when topic mismatch (mirror `decode_blob_chunk`).

- [ ] **Step 3: Roundtrip test**

Test proposal + partial borsh roundtrip; run:

`cargo test -p net vertex_gossip_roundtrip --locked`

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(net): vertex-proposal and vertex-partial gossip wire"
```

---

## Task 3: Swarm ingress for L1 host messages

**Files:**
- Modify: `crates/net/src/swarm_runner.rs`
- Modify: `crates/net/src/lib.rs` (re-export spawn struct fields if needed)

- [ ] **Step 1: Extend `spawn_gossip_tasks` signature**

Add optional channels (mirror blob):

```rust
vertex_proposals_tx: Option<mpsc::Sender<Vertex>>,
vertex_partials_tx: Option<mpsc::Sender<VertexPartial>>,
```

- [ ] **Step 2: In gossip `Message` handler**

After `decode_blob_chunk` attempt, try `decode_vertex_proposal` / `decode_vertex_partial`; `try_send` to channels; warn on full.

Order: blob chunk → vertex proposal/partial → `inbound_message` (existing consensus events).

- [ ] **Step 3: Compile**

`cargo check -p net --locked`

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(net): swarm fan-in for vertex proposal and partial"
```

---

## Task 4: `dag::cert` — aggregate from partials + stricter verify

**Files:**
- Modify: `crates/dag/src/cert.rs`
- Create: `crates/dag/tests/cert_from_partials.rs`

- [ ] **Step 1: Export `quorum_threshold`**

`pub fn quorum_threshold(n: u32) -> u32` (already exists as private — make `pub`).

- [ ] **Step 2: Add `build_cert_from_partials`**

```rust
pub fn build_cert_from_partials(
    vertex: &Vertex,
    valset: &ValidatorSet,
    contributors: &[(u32, BlsSig)], // valset index + sig
) -> Result<CertifiedVertex>
```

Reuse `aggregate_sigs` + bitmap build (same as `build_quorum_cert_with` but sigs supplied, no `sk_at`).

- [ ] **Step 3: Strengthen `verify_certified_vertex`**

Add after hash check:

1. `author_index(valset, vertex.author)` exists.
2. Author index bit set in certificate bitmap.
3. Existing ≥ `2f+1` + aggregate verify.

Add errors: `UnknownAuthor`, `AuthorNotInBitmap`.

- [ ] **Step 4: Unit tests**

`cert_from_partials.rs`: 4-valset, 3 partials including author → verify OK; missing author bit → fail.

Run: `cargo test -p dag cert_from_partials --locked`

- [ ] **Step 5: Fix breakages**

`cargo test -p dag --locked` and update any tests expecting old verify behavior.

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(dag): build CV from partials and require author in QC bitmap"
```

---

## Task 5: `LiveDag` helpers + `l1/propose.rs`

**Files:**
- Modify: `apps/node/src/live_dag.rs`
- Create: `apps/node/src/l1/propose.rs`
- Modify: `apps/node/src/l1/mod.rs`

- [ ] **Step 1: `LiveDag` methods**

```rust
pub fn certified_count_at_round(&self, round: Round) -> usize
pub fn max_certified_round(&self) -> Option<u64>
```

Count only in-memory index (or include DB — document: use `vertices_at_round` len).

- [ ] **Step 2: `propose.rs`**

```rust
pub fn may_propose_round(dag: &LiveDag, r: u64, valset: &ValidatorSet) -> bool;
pub fn parents_for_round(dag: &LiveDag, r: u64) -> Vec<Hash32>;
```

`parents_for_round`: for `r > 0`, all hashes at `r-1` sorted lexicographically; for `r == 0`, `vec![]`.

`may_propose_round`: `r == 0` → true; else `certified_count_at_round(r-1) >= quorum_threshold(n)`.

- [ ] **Step 3: Unit tests in `propose.rs` `#[cfg(test)]`**

Mock `LiveDag` with inserted CVs at round 1; gate for round 2 passes/fails.

Run: `cargo test -p node propose --locked`

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(node): L1 propose gate and multi-parent selection"
```

---

## Task 6: `CertCollector`

**Files:**
- Create: `apps/node/src/l1/cert_collector.rs`

- [ ] **Step 1: State**

```rust
struct CertCollector {
    valset: ValidatorSet,
    proposals: HashMap<Hash32, Vertex>,
    partials: HashMap<Hash32, HashMap<u32, BlsSig>>, // index -> sig
    published: HashSet<Hash32>, // dedup CV gossip
    publish_tx: mpsc::Sender<(Topic, Vec<u8>)>,
    events_tx: mpsc::Sender<Event>, // optional: emit CV locally without waiting for gossip loopback
    metrics: Arc<Metrics>,
}
```

- [ ] **Step 2: `on_proposal`**

- Verify `content_hash(vertex) == vertex.hash`.
- Verify `author ∈ valset`.
- Store in `proposals`.

- [ ] **Step 3: `on_partial`**

- Must have stored proposal for `vertex_hash`.
- Verify single partial: `verify(&pk, VERTEX_CERT, signing_bytes, sig)`.
- Insert partial; if distinct indices.len() >= quorum_threshold → `try_finalize`.

- [ ] **Step 4: `try_finalize`**

- `build_cert_from_partials` → `verify_certified_vertex` → `encode_certified_vertex` → `publish_tx`.
- Mark `published` hash; metric `vertex_cert_published_total`.

- [ ] **Step 5: Spawn loop**

`tokio::spawn` select on `proposals_rx`, `partials_rx`.

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(node): CertCollector aggregates L1 vertex partials"
```

---

## Task 7: `AuthorLoop`

**Files:**
- Create: `apps/node/src/l1/author_loop.rs`
- Modify: `apps/node/src/blob/mod.rs` (comment: `drain_pending` for AuthorLoop only)

- [ ] **Step 1: Struct**

```rust
pub struct AuthorLoop {
    self_id: ValidatorId,
    valset: ValidatorSet,
    dag: Arc<LiveDag>,
    signer: Arc<dyn SignerPort>, // or concrete DevSigner
    blob_custody: Option<BlobCustodyHandle>,
    publish_tx: mpsc::Sender<(Topic, Vec<u8>)>,
    next_round: u64,
    round_duration: Duration,
    metrics: Arc<Metrics>,
}
```

- [ ] **Step 2: `tick` logic**

1. `r = self.next_round`
2. If `!may_propose_round(&dag, r, &valset)` → return (keep `next_round`).
3. `parents = parents_for_round(&dag, r)`
4. `blobs = custody.drain_pending()` or `vec![]`
5. Build `Vertex { round: Round(r), author: self_id, parents, blobs, hash: zero }`
6. `dag::signing::seal_hash(&mut vertex)`
7. `sig = signer.sign_bls(dst::VERTEX_CERT, &signing_bytes(&vertex))`
8. `encode_vertex_proposal` + send; build `VertexPartial` + send
9. `self.next_round = r + 1` (only after successful publish)

**Do NOT** call `build_quorum_cert` for other validators.

- [ ] **Step 3: Self-partial into collector**

Either:
- (A) Also `try_send` partial to local `partials_tx` in runtime wiring, or
- (B) CertCollector hears own partial via gossip loopback.

Prefer **(A)** for lower latency — runtime connects author partial tx to collector.

- [ ] **Step 4: Unit test**

Mock dag with 3 CVs at round 0; tick proposes round 1 with 3 parents; no `build_quorum_cert` call.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(node): AuthorLoop proposes one vertex per tick"
```

---

## Task 8: Runtime wiring + config migration

**Files:**
- Modify: `apps/node/src/runtime.rs`
- Modify: `apps/node/src/config_layers.rs`
- Modify: `config/profiles/devnet.toml`
- Modify: `apps/node/src/observability/metrics.rs` (optional counters)

- [ ] **Step 1: Config rename**

`l1_driver_enabled` → `l1_author_loop_enabled` (serde alias `l1_driver_enabled` for one release optional).

When `l1_author_loop_enabled && network live`:
- Require `l1_real_vertex_certs = true` (panic or fail-fast at startup).

- [ ] **Step 2: Channels in `runtime.rs`**

```rust
let (proposal_tx, proposal_rx) = mpsc::channel(256);
let (partial_tx, partial_rx) = mpsc::channel(256);
```

Pass to `spawn_gossip_tasks` and `CertCollector::spawn`.

Wire `partial_tx` clone to `AuthorLoop` for self-partial.

- [ ] **Step 3: Spawn**

```rust
CertCollector::spawn(..., proposal_rx, partial_rx, publish_tx.clone(), ...);
AuthorLoop::spawn(..., publish_tx, blob_custody, ...);
```

**Remove** entire `if cfg.node.l1_driver_enabled { L1Driver::new ... }` block.

- [ ] **Step 4: Identity**

Resolve `self_id` from `cfg.node.identity.label` + valset (existing host bundle / devnet_keys).

- [ ] **Step 5: Manual smoke**

4-node compose: logs show proposals; CVs appear; `LiveDag` round advances.

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(node): wire AuthorLoop and CertCollector; drop L1Driver spawn"
```

---

## Task 9: Delete centralized produce path

**Files:**
- Delete: `apps/node/src/l1/driver.rs`
- Modify: `apps/node/src/l1/mod.rs`
- Delete: `apps/node/tests/l1_driver_smoke.rs`
- Modify: `apps/node/src/l1/vertex_builder.rs` — add `#![cfg(test)]` only helpers OR move to `dag` dev helpers

- [ ] **Step 1: Remove `pub mod driver` and `pub use L1Driver`**

- [ ] **Step 2: Grep guard**

`rg "L1Driver|build_quorum_vertices_with_blobs|l1_driver_enabled" apps/node`

Only `apps/sim` and `#[cfg(test)]` may retain batch helpers.

- [ ] **Step 3: Delete `parent.rs` usage from node** if unused.

- [ ] **Step 4: Full check**

`cargo test -p node --locked`
`cargo test -p net --locked`

- [ ] **Step 5: Commit**

```bash
git commit -m "refactor(node): remove centralized L1Driver produce path"
```

---

## Task 10: Integration tests

**Files:**
- Create: `apps/node/tests/l1_author_loop_smoke.rs`
- Create: `apps/node/tests/l1_distributed_qc.rs`
- Modify: `apps/node/tests/l1_gossip_roundtrip.rs`

- [ ] **Step 1: `l1_distributed_qc.rs` (2 swarms)**

Mirror `blob_gossip_roundtrip.rs`:
- Node A proposes round 0 vertex + partial
- Node B sends 2 more partials (use devnet keys for B,C labels)
- Node B collector publishes CV
- Assert `LiveDag` ingest + `verify_certified_vertex`

- [ ] **Step 2: `l1_author_loop_smoke.rs`**

Single process: mock `LiveDag` prefill round 0 with 3 CVs; `AuthorLoop` one tick → proposal on wire channel; collector finalizes.

- [ ] **Step 3: Blob attach test**

Submit blob on node A only → after tick, proposal contains `BlobRef` only in A's vertex (grep proposal bytes or hook test collector).

- [ ] **Step 4: Negative**

Partial with wrong sig rejected; `may_propose_round(2)` false when round 1 has 1 CV.

- [ ] **Step 5: Run full node tests**

`cargo test -p node --locked`

- [ ] **Step 6: Commit**

```bash
git commit -m "test(node): L1 distributed QC and author loop integration"
```

---

## Task 11: Docs + spec status

**Files:**
- Modify: `docs/superpowers/specs/2026-05-29-l1-distributed-vertex-cert-design.md`
- Modify: `docs/superpowers/plans/2026-05-23-06b-l1-vertex-driver.md` — add deprecation note at top

- [ ] **Step 1: Spec status**

`Approved (locked) — plan: 2026-05-29-l1-distributed-vertex-cert.md`

- [ ] **Step 2: README / devnet note** (if repo has operator doc)

Document: each validator process runs AuthorLoop; no single proposer.

- [ ] **Step 3: Commit**

```bash
git commit -m "docs: L1 distributed vertex cert plan and deprecate central driver"
```

---

## Verification checklist (end state)

- [ ] `rg build_quorum_cert apps/node/src` — only in tests or absent
- [ ] `rg L1Driver apps/node` — empty
- [ ] 4-node devnet: round `r` has multiple authors; round `r+1` parents.len() >= 3
- [ ] `cargo test -p node -p net -p dag --locked` green
- [ ] Orchestrator still rejects bad CV (`vertex_cert_rejected` metric increments on tamper test)

---

## Task dependency graph

```text
Task 1 (types)
  → Task 2 (wire) → Task 3 (swarm)
  → Task 4 (cert)
Task 5 (propose + LiveDag) ─┐
Task 6 (collector)        ─┼→ Task 7 (author) → Task 8 (runtime) → Task 9 (delete) → Task 10 (tests) → Task 11 (docs)
         ↑──────────────────┘
```

**Suggested PR split (optional):**

1. PR1: Tasks 1–4 (types + wire + cert) — no node behavior change  
2. PR2: Tasks 5–8 (node loops + runtime)  
3. PR3: Tasks 9–11 (delete driver + tests + docs)

---

## Metrics (optional in Task 8)

| Metric | When |
|--------|------|
| `vertex_proposals_total` | AuthorLoop publish proposal |
| `vertex_partials_received_total` | Collector ingress |
| `vertex_cert_published_total` | Collector publishes CV |
| `vertex_propose_skipped_total` | Gate failed at tick |

Existing `vertex_cert_rejected_total` unchanged (orchestrator).
