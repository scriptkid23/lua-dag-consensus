# Distributed Vertex Certificate Protocol — Design

**Date:** 2026-06-04
**Status:** Approved (design) — implementation plan: [`2026-06-11-distributed-vertex-certificate.md`](../plans/2026-06-11-distributed-vertex-certificate.md)
**Topic:** Replace the host-side devnet vertex-certificate factory with a real distributed "collect ≥2f+1 BLS partials from peers" protocol.

## Problem

Layer 1's control plane currently **simulates** vertex certification. A single host
fabricates `2f+1` certified vertices per micro-round and signs them with *every*
validator's devnet secret key:

- `apps/node/src/l1/vertex_builder.rs` — "Deterministic devnet certified-vertex factory".
  `build_quorum_vertices_with_blobs` builds `2f+1` vertices, one per author, and
  `build_certified_vertex_with_blobs` calls `dag::cert::build_quorum_cert`, which resolves
  each signer's secret key via `devnet::devnet_bls_ikm` (host holds all keys).
- `apps/node/src/l1/driver.rs::tick_round` runs on a fixed timer, ingests the fabricated
  batch into `LiveDag`, and broadcasts whole already-certified vertices.

This diverges from the target architecture diagram, which describes: a proposer broadcasts
a vertex header → peers each verify and return a BLS partial → the proposer aggregates
≥2f+1 partials into a `CertifiedVertex`. No partial collection over gossip exists today.

The BLS aggregate itself is real (`verify_certified_vertex` checks a 2f+1 threshold), but
the *collection* is host-side and not distributed.

## Goals

1. Each node proposes **its own** vertex per round (`author = self`), broadcasts the header,
   collects ≥2f+1 partial BLS signatures from peers, and aggregates them into a
   `CertifiedVertex` — the standard Narwhal model. One node = one author per round.
2. Eliminate the anti-pattern where one host holds every validator's secret key.
3. Cert-driven round advancement (Narwhal): advance to round `r+1` only after holding
   ≥2f+1 certificates from round `r`, used as the new vertex's parents. Timer is a fallback.
4. First cut includes: happy-path collect+aggregate, timeout/round-advancement handling,
   proposer equivocation detection (with slashing), and anti-spam (verify peer partials,
   dedup, bound memory).

## Non-Goals (deferred)

- Voter (partial) equivocation slashing — first cut only drops + emits a metric.
- Generalizing macro_fin + vertex_cert into a shared `quorum::Aggregator<Item>` engine
  (premature abstraction; revisit once two concrete implementations exist — YAGNI).
- Changing how Bullshark/Layer 2 consumes the DAG. `DagView` (`LiveDag`) and the
  `Event::CertifiedVertexReceived` handoff are unchanged. `causal_set` remains an RPC.

## Chosen Approach

**Approach A — new `vertex_cert` module in `crates/consensus`, mirroring `macro_fin` 1:1.**

The macro-finalization layer already implements this exact shape — distributed partial
collection and aggregation — via `Action::BroadcastBlsPartial` → `BroadcastSubnetAggregate`
→ `BroadcastMacroQc`, `SignerPort::sign_bls`, equivocation via `proposals_seen`, and
`slashing::equivocation`. We mirror that proven, sim-tested pattern rather than abstracting
it (Approach B, deferred) or keeping logic host-side (rejected — not deterministic/sim-testable).

The consensus state machine stays deterministic: `step(Event, ctx) -> Result<Actions>`.

## Section 1 — Wire protocol & data types

New message types in `crates/types/src/dag.rs`:

```rust
/// Header a node proposes for its own round (not yet certified).
pub struct VertexProposal {
    pub vertex: Vertex,        // author = self; parents = 2f+1 cert hashes of round-1
    pub proposer_sig: BlsSig,  // signs signing_bytes(vertex) under DST VERTEX_PROPOSAL
}

/// A single validator's partial vote on a proposal.
pub struct VertexPartial {
    pub vertex_hash: Hash32,   // hash of the voted vertex
    pub round: Round,
    pub author: ValidatorId,   // proposal owner (routing/aggregation)
    pub voter: ValidatorId,    // partial signer
    pub sig: BlsSig,           // signs signing_bytes(vertex) under dst::VERTEX_CERT
}
```

`CertifiedVertex` is unchanged — it is the final output and already has
`verify_certified_vertex`.

New `Event` variants (`crates/consensus/src/event.rs`):

```rust
Event::VertexProposalReceived(VertexProposal)
Event::VertexPartialReceived(VertexPartial)
// CertifiedVertexReceived already exists → still feeds bullshark
```

New `Action` variants (`crates/consensus/src/action.rs`):

```rust
Action::BroadcastVertexProposal(VertexProposal)
Action::BroadcastVertexPartial(VertexPartial)
Action::BroadcastCertifiedVertex(CertifiedVertex)  // replaces host directly encoding
// reuse: ScheduleTimer / CancelTimer / EmitSlashEvidence
```

New `Topic` variants (`crates/net`):

```rust
Topic::VertexProposal   // -> Event::VertexProposalReceived
Topic::VertexPartial    // -> Event::VertexPartialReceived
Topic::CertifiedVertex  // ALREADY EXISTS; now emitted from an Action, not host-direct
```

New DST `crypto::hash::dst::VERTEX_PROPOSAL` (propose authority). `VERTEX_CERT` is reused
for partial and aggregate so `verify_certified_vertex` matches.

**Key design point:** a partial signs *exactly the same message* (`signing_bytes(vertex)`
under `VERTEX_CERT`) that `build_quorum_cert_with` / `verify_certified_vertex` already use,
so aggregation is just collecting partial sigs + setting the bitmap — no crypto change.

Design decisions confirmed:
- `proposer_sig` (propose authority) is kept separate from the partial vote.
- Message types live in `crates/types/src/dag.rs`.

## Section 2 — `VertexBook` state + handlers

New module `crates/consensus/src/vertex_cert/` (mirrors `macro_fin/`). Per-validator,
in-memory state:

```rust
pub struct VertexBook {
    self_id: ValidatorId,
    current_round: Round,

    // Quorum-certified vertices, grouped by round → parents for the next round.
    certified_by_round: HashMap<Round, Vec<CertifiedVertex>>,

    // Partials being collected for THIS node's proposal: vertex_hash -> {voter -> sig}.
    collecting: HashMap<Hash32, BTreeMap<ValidatorId, BlsSig>>,

    // Proposals seen per (round, author) → equivocation detection.
    proposals_seen: HashMap<(Round, ValidatorId), Vec<VertexProposal>>,

    // Which (round, author) this node already voted for → 1 vote / proposer / round.
    voted: HashSet<(Round, ValidatorId)>,

    round_timer: Option<TimerId>,   // fallback timeout for the current round
}
```

Three primary handlers, all returning `Result<Actions>`, deterministic:

**`on_vertex_proposal(book, cfg, p, ctx)`** — inbound header from a peer:
1. Verify `proposer_sig` (DST `VERTEX_PROPOSAL`) + that `vertex.author` is the signer +
   `vertex.hash == content_hash(vertex)`. Bad → drop, bump `rejected_crypto`.
2. Equivocation check via `proposals_seen` (mirrors macro_fin): same `(round, author)`
   but different `vertex.hash` → `Action::EmitSlashEvidence`, do not vote.
3. Verify parents: `vertex.parents` must be valid cert hashes at `round-1` (checked via
   `ctx.dag` / `certified_by_round`). Missing → hold (do not vote yet).
4. If not yet `voted` for `(round, author)` → sign partial
   (`ctx.signer.sign_bls(VERTEX_CERT, signing_bytes(vertex))`), set `voted`, emit
   `Action::BroadcastVertexPartial`.

**`on_vertex_partial(book, cfg, bp, ctx)`** — proposer collects votes for its own vertex:
1. Ignore if `bp.author != self_id` (only the proposer aggregates its own vertex) or
   `vertex_hash` is not one of this node's proposals.
2. **Verify the peer's partial signature** before accepting (DST `VERTEX_CERT`, pubkey
   from valset) — rejects forged partials. Dedup by `voter` in `collecting`.
3. When `collecting[h].len() >= quorum_threshold(n)` (2f+1): assemble a `CertifiedVertex`
   from the collected sigs (see `assemble_cert` below), verify once with
   `verify_certified_vertex`, emit `Action::BroadcastCertifiedVertex`, and self-ingest the
   cert into `certified_by_round` (enables round advancement).

**`on_certified_vertex(book, cfg, cv, ctx)`** — in addition to feeding Bullshark, vertex_cert
observes certs to update `certified_by_round` (peer-certified vertices from other proposers);
reaching 2f+1 triggers proposing the next round (Section 3).

**`assemble_cert` refactor:** `dag::cert::build_quorum_cert_with` currently takes an
`FnMut(u32) -> SecretKey` resolver and *signs itself*. Extract the "collect sigs + set
bitmap" part into a helper `assemble_cert(vertex, valset, &[(idx, BlsSig)]) -> CertifiedVertex`
that both the devnet path and the distributed path call. Small change, preserves the old API.

Design decisions confirmed:
- Only the **proposer** aggregates its own vertex; peers only sign partials and receive the
  final cert.
- `assemble_cert` is split out of `build_quorum_cert_with`.

## Section 3 — Cert-driven round advancement + timeout

This is the largest behavioral change (replaces the L1Driver timer loop).

**Round advancement rule (Narwhal):**
- A node proposes a vertex for round `r+1` only after
  `certified_by_round[r].len() >= 2f+1`.
- When the condition becomes true (in `on_vertex_partial` after self-certifying, or in
  `on_certified_vertex` when receiving a peer cert):
  1. `current_round = r+1`.
  2. `parents = certified_by_round[r]` (first 2f+1 hashes, deterministic by author order).
  3. Drain pending blobs into the vertex (Section 5).
  4. `seal_hash(vertex)`, sign `proposer_sig`, emit `Action::BroadcastVertexProposal` and
     **self-vote** the partial (seed `collecting` with self's sig).
  5. Cancel the old round timer, `ScheduleTimer` a fresh timeout for round `r+1`.

**Pure helper:** `fn maybe_advance(book, cfg, ctx) -> Result<Actions>` — called after each
new cert is added to `certified_by_round`. Idempotent: if `current_round` already > r, no-op
(prevents double-propose).

**Bootstrap round 0:** no round `-1` for parents. `vertex_cert::genesis_propose(book, ctx)`
is called once at startup → proposes a round-0 vertex with `parents = []`. L1Driver calls
this instead of the current fabrication loop.

**Timeout / liveness when < 2f+1:**
- Each round has a `round_timer` (`ScheduleTimer { delay = round_duration }`, reusing
  `bullshark` `TimerScheduler`).
- `on_round_timer_fired(book, cfg, ctx)`: if the round still lacks 2f+1 certs:
  - **Do not jump rounds** (unsafe — parents must be certified). Instead re-broadcast this
    node's proposal + partial (loss recovery) and re-arm the timer (linear backoff, capped).
  - Bump `vertex_round_stalled` metric. The round advances only when enough certs exist,
    preserving the invariant "parents are always certified."
- Pending blobs of a stalled round **stay in the PQ**; blobs are only drained when *starting*
  to build a proposal. A stalled proposal keeps re-broadcasting the same vertex (immutable
  across retries within a round), so the blobs ride along and are never lost.

**Wave/Bullshark unchanged:** `on_certified_vertex` still feeds Bullshark; vertex_cert only
runs *before* it to produce certs. Bullshark still reads the DAG via `DagView` (`LiveDag`).

**Noted risk:** under "no self-jump," if the network fragments below 2f+1 online, the DAG
stops advancing (correct by theory — cannot safely advance). This is expected behavior;
covered by a partition test.

Design decision confirmed: timeout = re-broadcast + hold, **no self-jump** — preserves the
Narwhal invariant.

## Section 4 — Equivocation detection & anti-spam/verify

Reuses existing slashing infrastructure (`SlashEvidence`, `Action::EmitSlashEvidence`,
`slashing::equivocation`).

**Equivocation (double-propose):**
- In `on_vertex_proposal`, `proposals_seen[(round, author)]`:
  - A second proposal for the same `(round, author)` with a **different `vertex.hash`** is
    equivocation evidence. Emit:
    ```rust
    Action::EmitSlashEvidence {
        offender: author,
        evidence: SlashEvidence::VertexEquivocation { round, first, second },
    }
    ```
  - Add a `SlashEvidence::VertexEquivocation` variant (parallel to the existing
    `MacroEquivocation`). It carries both signed proposals → anyone can verify offline.
  - A duplicate with the **same hash** is idempotent re-broadcast → ignore.
- ActionApplier persists evidence via `append_slash_evidence` + gossips via the existing
  `Topic::SlashEvidence`.

**Anti-spam / verify partial:**
1. **Verify before accepting:** every `VertexPartial` must pass BLS verification (DST
   `VERTEX_CERT`, pubkey from valset by `voter`) *before* entering `collecting`. Bad sig →
   drop + bump `vertex_partial_rejected`.
2. **Voter must be in the valset** for the current epoch → else drop.
3. **Dedup:** `collecting[h]` is a `BTreeMap<ValidatorId, BlsSig>` → one voter counts once
   even on repeats. A voter sending two different sigs for the same vertex → keep the first,
   bump `vertex_partial_conflict` (no slash in first cut).
4. **Memory bound:** only collect partials for vertices at `current_round` (and the
   immediately prior round for network-delay tolerance). Partials for too-old/too-future
   rounds → drop. Prevents flood-driven memory blow-up.
5. **Proposal verify:** bad `proposer_sig`, mismatched `author`, or
   `hash != content_hash` → drop immediately (Section 2 step 1).

**Voter (partial) equivocation:** detected only at this node via
`voted: HashSet<(round, author)>`. Detecting/slashing *other* voters' partial-equivocation
needs a `VertexVoteBook` and is **deferred**: first cut drops + emits a metric. The "equivocation
detection" scope item is interpreted as **proposer double-propose** (with real slashing);
voter-equivocation is detect+metric only.

Design decisions confirmed:
- Add `SlashEvidence::VertexEquivocation` for double-propose (with slashing).
- Voter-equivocation is drop+metric in the first cut; slashing deferred.

## Section 5 — Host integration (apps/node)

**`StateMachine::step` extended** — route the two new events (logically *before* the
`CertifiedVertexReceived` branch):

```rust
Event::VertexProposalReceived(p) => vertex_cert::on_vertex_proposal(&mut self.vertices, &cfg, p, ctx),
Event::VertexPartialReceived(bp)  => vertex_cert::on_vertex_partial(&mut self.vertices, &cfg, bp, ctx),
Event::CertifiedVertexReceived(cv) => {
    let mut actions = vertex_cert::on_certified_vertex(&mut self.vertices, &cfg, &cv, ctx)?; // certified_by_round + maybe_advance
    merge(&mut actions, bullshark::on_certified_vertex(...));   // UNCHANGED
    macro_fin::on_local_micro_qcs(...);                          // UNCHANGED
    Ok(actions)
}
Event::TimerFired(id) => { /* add vertex_cert::on_timer_fired alongside bullshark/macro_fin */ }
```

`StateMachine` gains one field: `vertices: VertexBook`.

**L1Driver shrinks** (`apps/node/src/l1/driver.rs`) — the largest host-side change:
- **Remove** `build_quorum_vertices_with_blobs` from the production path (per-tick fabrication
  of 2f+1 vertices). `vertex_builder.rs` remains for test/sim devnet only.
- `tick_round` no longer self-builds/ingests vertices. The driver only:
  1. At start: calls `vertex_cert::genesis_propose` (round 0) → pushes the Action outward.
  2. No self-jumping timer; the round timer is owned by `vertex_cert` via
     `Action::ScheduleTimer` (host just relays into the TimerRegistry, as macro_fin does).
- L1Driver effectively dissolves into the shared event loop; keep a thin `L1Bootstrap` only
  to seed genesis and wire `BlobCustodyHandle`.

**Drain pending blobs:** `drain_pending()` moves from "each driver tick" to "when vertex_cert
builds a proposal for a new round." Since vertex_cert lives in the consensus crate (and does
not know `BlobCustodyHandle`), add a small port `PendingBlobSource` to `HostContext`
(alongside `dag`/`signer`/`beacon`); vertex_cert calls `ctx.pending_blobs.drain()` when
building a proposal. The host plugs `BlobCustodyHandle` into this port.

**Outbound wiring** (`gossip_wire.rs::outbound_broadcast`): add three Action→Topic branches.
Inbound `inbound_message`: add two Topic→Event branches.

**Signer:** each node already has a `ValidatorSigner` implementing `SignerPort`. vertex_cert
signs via `ctx.signer.sign_bls(...)` — it never needs another validator's key. This is the
point where the "one host holds all keys" anti-pattern is removed.

**ActionApplier:** `BroadcastVertex*` are broadcast-only → handled by `outbound_broadcast`;
ActionApplier no-ops (existing `_ => {}`). Only `EmitSlashEvidence` persists (already wired).

**Config / rollout flag:** add `[node] vertex_protocol = "distributed" | "devnet_factory"`
(default `devnet_factory` so existing CI/sim stays green; set `distributed` for real nodes).
Enables incremental merge and A/B.

Design decisions confirmed:
- Add a `PendingBlobSource` port to `HostContext` (consensus pulls blobs; host does not push).
- Keep a `vertex_protocol` flag, default `devnet_factory` (safe rollout).

## Section 6 — Testing strategy

**Unit (`consensus`, `vertex_cert/` inline, like macro_fin):**
- `on_vertex_proposal`: valid → exactly one partial; bad sig → drop; parents not certified
  → hold (0 actions); double-propose different hash → `EmitSlashEvidence`; same-hash
  re-broadcast → idempotent.
- `on_vertex_partial`: 2f+1 collected → emit `BroadcastCertifiedVertex` and that cert
  **verifies** with `verify_certified_vertex`; forged partial → drop; voter dedup; below
  quorum → no cert yet.
- `maybe_advance`: 2f+1 certs for round r → propose round r+1 with correct parents; below →
  no-op; idempotent (no double-propose).
- `on_timer_fired`: stalled round → re-broadcast, no round jump; "parents always certified"
  invariant holds.

**Integration (apps/node, like `l1_driver_smoke.rs`):**
- Genesis → round-0 proposal → partial → cert → advance to round 1; assert the DAG holds the
  right number of certs.
- Wire round-trip: `VertexProposal`/`VertexPartial` encode→decode via `inbound_message` to the
  right Event (like `certified_vertex_roundtrips_on_wire`).

**Sim end-to-end (apps/sim, like `scenarios_l2.rs`):**
- **Happy path n=4:** multiple rounds, every vertex certifies, Bullshark commits as before →
  checkpoint hash matches baseline.
- **Partition / liveness:** only 2/4 nodes online (< 2f+1) → DAG halts (correct by theory),
  no panic, no forged certs. Restore quorum → resumes.
- **Equivocation:** one node proposes two different vertices in the same round → other nodes
  emit `VertexEquivocation` evidence.
- **Blob liveness:** blob submitted via RPC → appears in a certified vertex → `lua_blobStatus`
  reports available.

**Regression:** full suite with `vertex_protocol = devnet_factory` (default) must stay green,
proving the change does not break the old path; a separate suite runs `distributed`.

**Invariants this fixes:** (1) certificates are now collected from real peers (not a host-side
factory); (2) `causal_set` stays an RPC; L1→L2 still flows via `Event::CertifiedVertexReceived`.

## Files Touched (summary)

| Area | Change |
|---|---|
| `crates/types/src/dag.rs` | `VertexProposal`, `VertexPartial` types |
| `crates/consensus/src/event.rs` | 2 new `Event` variants |
| `crates/consensus/src/action.rs` | 3 new `Action` variants |
| `crates/consensus/src/vertex_cert/` | new module: `VertexBook` + handlers |
| `crates/consensus/src/state_machine.rs` | route new events; add `vertices` field |
| `crates/consensus/src/host_context.rs` | add `PendingBlobSource` port |
| `crates/consensus/src/slashing/...` | `SlashEvidence::VertexEquivocation` |
| `crates/dag/src/cert.rs` | extract `assemble_cert` |
| `crates/dag/src/signing.rs` / `crypto dst` | `VERTEX_PROPOSAL` DST |
| `crates/net/.../topic`, `gossip_wire.rs` | 2 topics, inbound/outbound mapping |
| `apps/node/src/l1/driver.rs` | shrink to bootstrap; remove fabrication from prod path |
| `apps/node/src/blob/mod.rs` | implement `PendingBlobSource` over `BlobCustodyHandle` |
| `apps/node/src/config_layers.rs` | `vertex_protocol` flag |
| tests: `consensus`, `apps/node`, `apps/sim` | as Section 6 |
