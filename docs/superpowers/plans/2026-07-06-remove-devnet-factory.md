# Remove `devnet_factory` (Distributed-Only L1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Delete the legacy `vertex_protocol = "devnet_factory"` path (host-side `L1Driver` fabricating 2f+1 quorum certs with devnet keys) so the distributed `vertex_cert` protocol is the only L1 vertex production path.

**Architecture:** Node config loses the `VertexProtocol` enum and the `vertex_protocol` / `l1_driver_enabled` / `l1_real_vertex_certs` flags. The runtime derives `propose_enabled` from gossip-swarm presence (skeleton mode = ingress-only). The orchestrator always verifies certificates. `apps/node/src/l1/` (driver, vertex_builder, parent) is deleted; the two tests that used `vertex_builder` fabricate certs via `dag::cert` instead. `apps/sim` is untouched.

**Tech Stack:** Rust workspace (cargo), tokio, libp2p gossipsub, `dag::cert` / `dag::signing` BLS helpers, serde/toml config layers.

**Spec:** [`docs/superpowers/specs/2026-07-06-remove-devnet-factory-design.md`](../specs/2026-07-06-remove-devnet-factory-design.md) (Approved).

## Global Constraints

- `cargo test --workspace --locked` must be green after every task.
- `apps/node/src/` must never construct a quorum certificate itself after this plan; only `crates/consensus::vertex_cert` assembles certs on live nodes. Test code may use `dag::cert::build_quorum_cert`.
- `apps/sim/` is out of scope — do not modify anything under it (its `vertex_factory.rs` is a sim-internal fixture and stays).
- `l1_blob_custody_enabled`, `blob_chunk_size_bytes`, and the `erasure_*` keys stay in config — only the three vertex flags are removed.
- Windows dev box: run commands from repo root `d:\1hoodlabs\lua-dag-consensus`.

---

### Task 1: Port `vertex_cert_reject.rs` off `vertex_builder`

The test currently imports `node::l1::vertex_builder::{build_certified_vertex, sim_vertex_hash}`. Rebuild the same three coverage points (unsealed-hash reject, sealed-body-with-garbage-signature reject, real-cert accept) using only `dag::cert` + `dag::signing`.

**Files:**
- Modify: `apps/node/tests/vertex_cert_reject.rs` (entire file — replace with the content below)

**Interfaces:**
- Consumes: `dag::cert::{build_quorum_cert, verify_certified_vertex}` (`crates/dag/src/cert.rs:44,` verify below builder), `dag::signing::seal_hash` (`crates/dag/src/signing.rs:40`), `node::devnet_keys::devnet_valset_four`.
- Produces: nothing used by later tasks; after this task nothing in `tests/vertex_cert_reject.rs` references `node::l1`.

- [ ] **Step 1: Replace the test file**

Replace the full contents of `apps/node/tests/vertex_cert_reject.rs` with:

```rust
//! Tampered certified vertices must never verify.

use dag::{cert, signing};
use node::devnet_keys::devnet_valset_four;
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    dag::{CertifiedVertex, Vertex},
    primitives::Round,
};

fn fixture_certificate() -> BlsAggSig {
    BlsAggSig {
        sig: BlsSig([0xAB; 96]),
        bitmap: vec![0xFF],
    }
}

#[test]
fn unsealed_hash_fails_verify() {
    let valset = devnet_valset_four();
    let author = valset.entries[0].id;
    let vertex = Vertex {
        round: Round(0),
        author,
        parents: vec![],
        blobs: vec![],
        hash: Hash32([0x11; 32]), // not the sealed content hash
    };
    let cv = CertifiedVertex {
        vertex,
        certificate: fixture_certificate(),
    };
    assert!(cert::verify_certified_vertex(&cv, &valset).is_err());
}

#[test]
fn sealed_body_with_fixture_signature_fails_bls_verify() {
    let valset = devnet_valset_four();
    let author = valset.entries[0].id;
    let mut vertex = Vertex {
        round: Round(1),
        author,
        parents: vec![],
        blobs: vec![],
        hash: Hash32([0u8; 32]),
    };
    signing::seal_hash(&mut vertex);
    let cv = CertifiedVertex {
        vertex,
        certificate: fixture_certificate(),
    };
    assert!(cert::verify_certified_vertex(&cv, &valset).is_err());
}

#[test]
fn real_quorum_cert_verifies() {
    let valset = devnet_valset_four();
    let author = valset.entries[0].id;
    let mut vertex = Vertex {
        round: Round(2),
        author,
        parents: vec![],
        blobs: vec![],
        hash: Hash32([0u8; 32]),
    };
    signing::seal_hash(&mut vertex);
    let cv = cert::build_quorum_cert(&vertex, &valset, &[0, 1, 2]).expect("quorum cert builds");
    cert::verify_certified_vertex(&cv, &valset).expect("real cert must verify");
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p node --test vertex_cert_reject --locked`
Expected: PASS — `test result: ok. 3 passed`

- [ ] **Step 3: Commit**

```bash
git add apps/node/tests/vertex_cert_reject.rs
git commit -m "test(node): port vertex_cert_reject off l1::vertex_builder"
```

---

### Task 2: Port `l1_gossip_roundtrip.rs` off `vertex_builder`

Replace the single `build_quorum_vertices_for_valset(...)` call with one real cert built via `dag::cert`. The rest of the two-swarm roundtrip test is unchanged.

**Files:**
- Modify: `apps/node/tests/l1_gossip_roundtrip.rs:12-15` (imports) and `:59-63` (cert construction)

**Interfaces:**
- Consumes: same `dag::cert` / `dag::signing` helpers as Task 1.
- Produces: after this task nothing in `apps/node/tests/` references `node::l1` except `l1_driver_smoke.rs` (deleted in Task 3).

- [ ] **Step 1: Update imports**

Replace (lines 12-15):

```rust
use node::{
    devnet_keys::devnet_valset_four,
    l1::vertex_builder::build_quorum_vertices_for_valset,
};
```

with:

```rust
use dag::{cert, signing};
use node::devnet_keys::devnet_valset_four;
use types::{
    crypto_types::Hash32,
    dag::Vertex,
    primitives::Round,
};
```

(`use consensus::event::Event;` and the other existing imports stay.)

- [ ] **Step 2: Replace the cert construction**

Replace (lines 59-63):

```rust
    let valset = devnet_valset_four();
    let cv = build_quorum_vertices_for_valset(0, &valset, None, true)
        .into_iter()
        .next()
        .expect("quorum batch non-empty");
```

with:

```rust
    let valset = devnet_valset_four();
    let mut vertex = Vertex {
        round: Round(0),
        author: valset.entries[0].id,
        parents: vec![],
        blobs: vec![],
        hash: Hash32([0u8; 32]),
    };
    signing::seal_hash(&mut vertex);
    let cv = cert::build_quorum_cert(&vertex, &valset, &[0, 1, 2]).expect("quorum cert builds");
```

- [ ] **Step 3: Run the test**

Run: `cargo test -p node --test l1_gossip_roundtrip --locked`
Expected: PASS — `test result: ok. 1 passed` (spawns two loopback swarms; allow ~15s)

- [ ] **Step 4: Commit**

```bash
git add apps/node/tests/l1_gossip_roundtrip.rs
git commit -m "test(node): port l1_gossip_roundtrip off l1::vertex_builder"
```

---

### Task 3: Distributed-only runtime and orchestrator

Delete the `vertex_protocol` match (both arms, including the `L1Driver` spawn). Derive `propose_enabled` from swarm presence. Orchestrator: drop `l1_real_vertex_certs` (always verify), rename `vertex_protocol_distributed` → `propose_enabled`. Update the one surviving `Orchestrator::new` test call site; delete the driver smoke test.

**Files:**
- Modify: `apps/node/src/runtime.rs:15-25` (use block), `:224-285` (match + orchestrator construction)
- Modify: `apps/node/src/orchestrator.rs:36-73` (struct + `new`), `:106-135` (`run` gates)
- Modify: `apps/node/tests/l1_distributed_smoke.rs:99-111`
- Delete: `apps/node/tests/l1_driver_smoke.rs`

**Interfaces:**
- Consumes: `dag::cert::verify_certified_vertex(&CertifiedVertex, &ValidatorSet)` (already imported path style in orchestrator).
- Produces: `Orchestrator::new(sm, bridge, events_rx, persistence, metrics, net_actions_tx, host_bundle, action_applier, valset, propose_enabled: bool)` — 10 params, used by `runtime.rs` and `l1_distributed_smoke.rs`. Runtime local `let propose_enabled = gossip_publish_tx.is_some();`.

- [ ] **Step 1: Orchestrator struct + constructor**

In `apps/node/src/orchestrator.rs`, replace the last three struct fields (lines 37-41):

```rust
    valset: ValidatorSet,
    l1_real_vertex_certs: bool,
    /// Distributed L1 path active: genesis-propose at startup and loop
    /// own certified vertices back as local events.
    vertex_protocol_distributed: bool,
```

with:

```rust
    valset: ValidatorSet,
    /// Propose own vertices: genesis-propose at startup and loop own
    /// certified vertices back as local events. `false` only in skeleton
    /// mode (no gossip swarm) — the node then runs ingress-only.
    propose_enabled: bool,
```

In `new()`, replace the parameters `l1_real_vertex_certs: bool, vertex_protocol_distributed: bool` with the single `propose_enabled: bool`, and in the `Self { ... }` literal replace `l1_real_vertex_certs,` and `vertex_protocol_distributed,` with `propose_enabled,`.

- [ ] **Step 2: Orchestrator run-loop gates**

In `run()` (line 108), change the genesis gate:

```rust
        if self.vertex_protocol_distributed {
```

to:

```rust
        if self.propose_enabled {
```

Then make the verify gate unconditional — replace (lines 124-135):

```rust
                    if let Event::CertifiedVertexReceived(cv) = &event {
                        if self.l1_real_vertex_certs {
                            if let Err(e) = dag::cert::verify_certified_vertex(cv, &self.valset) {
                                warn!(
                                    target: "node::orchestrator",
                                    error = %e,
                                    "rejecting certified vertex"
                                );
                                self.metrics.vertex_cert_rejected.inc();
                                continue;
                            }
                        }
```

with:

```rust
                    if let Event::CertifiedVertexReceived(cv) = &event {
                        if let Err(e) = dag::cert::verify_certified_vertex(cv, &self.valset) {
                            warn!(
                                target: "node::orchestrator",
                                error = %e,
                                "rejecting certified vertex"
                            );
                            self.metrics.vertex_cert_rejected.inc();
                            continue;
                        }
```

(The `host_bundle.dag.ingest` block that follows is unchanged.)

- [ ] **Step 3: Runtime — replace the protocol match**

In `apps/node/src/runtime.rs`, delete `l1::L1Driver,` from the `use crate::{...}` block (line 21).

Replace the whole `match cfg.node.vertex_protocol { ... }` block (lines 224-270) with:

```rust
    // L1 vertex production: distributed vertex_cert protocol. Active only
    // with a live gossip swarm; skeleton mode runs ingress-only.
    let propose_enabled = gossip_publish_tx.is_some();
    if propose_enabled {
        info!(target: "node", "L1 distributed vertex certification active");
    } else {
        info!(
            target: "node",
            "no gossip swarm (skeleton mode): ingress-only, vertex production disabled"
        );
    }
```

Then update the orchestrator construction (lines 273-285) — the two boolean arguments

```rust
        cfg.node.l1_real_vertex_certs,
        cfg.node.vertex_protocol == crate::config_layers::VertexProtocol::Distributed,
```

become:

```rust
        propose_enabled,
```

- [ ] **Step 4: Update `l1_distributed_smoke.rs`, delete `l1_driver_smoke.rs`**

In `apps/node/tests/l1_distributed_smoke.rs` (lines 99-111), the `Orchestrator::new(...)` call ends with `valset, true, true,` — change to `valset, true,`.

Delete the file `apps/node/tests/l1_driver_smoke.rs` (its only subject is `L1Driver`).

- [ ] **Step 5: Build and run node tests**

Run: `cargo test -p node --locked`
Expected: PASS — all node unit + integration tests green; `l1_driver_smoke` no longer listed.

Note: `apps/node/src/l1/` still exists and compiles standalone at this point; it is deleted in Task 4.

- [ ] **Step 6: Commit**

```bash
git add apps/node/src/runtime.rs apps/node/src/orchestrator.rs apps/node/tests/l1_distributed_smoke.rs
git rm apps/node/tests/l1_driver_smoke.rs
git commit -m "feat(node): distributed-only L1 production, always verify certs"
```

---

### Task 4: Delete the `apps/node/src/l1/` module

Nothing references it after Tasks 1-3.

**Files:**
- Delete: `apps/node/src/l1/driver.rs`, `apps/node/src/l1/vertex_builder.rs`, `apps/node/src/l1/parent.rs`, `apps/node/src/l1/mod.rs`
- Modify: `apps/node/src/lib.rs:17` (remove `pub mod l1;`)
- Modify: `apps/node/src/blob/mod.rs:86` (stale doc comment)

**Interfaces:**
- Consumes: nothing.
- Produces: `node::l1` no longer exists; any future reference is a compile error.

- [ ] **Step 1: Verify nothing references the module**

Run: `rg "node::l1|crate::l1|l1::" apps/node --glob '!**/l1/**'`
Expected: no matches. (If there are matches, a Task 1-3 step was missed — fix that first.)

- [ ] **Step 2: Delete the module**

```bash
git rm -r apps/node/src/l1
```

Then remove line 17 `pub mod l1;` from `apps/node/src/lib.rs`.

In `apps/node/src/blob/mod.rs:86`, update the stale doc comment:

```rust
    /// Pop every queued `BlobRef` in FIFO order. Called by `L1Driver` each tick.
```

becomes:

```rust
    /// Pop every queued `BlobRef` in FIFO order. Drained by the `vertex_cert`
    /// proposer (via `PendingBlobSource`) when building this node's proposal.
```

- [ ] **Step 3: Build and test**

Run: `cargo test -p node --locked`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/node/src/lib.rs apps/node/src/blob/mod.rs
git commit -m "refactor(node): delete legacy L1Driver/vertex_builder module"
```

---

### Task 5: Remove the config flags and clean `devnet.toml`

**Files:**
- Modify: `apps/node/src/config_layers.rs:23-32` (enum), `:60-70` (fields)
- Modify: `config/profiles/devnet.toml:23-30`

**Interfaces:**
- Consumes: nothing.
- Produces: `NodeSection` without `l1_driver_enabled` / `l1_real_vertex_certs` / `vertex_protocol`; no `VertexProtocol` type. Stale keys in user TOML are silently ignored (no `deny_unknown_fields` — accepted per spec).

- [ ] **Step 1: Remove the enum and fields**

In `apps/node/src/config_layers.rs` delete the `VertexProtocol` enum (lines 23-32 including its doc comment):

```rust
/// L1 vertex production path selector (06-04 design §5).
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VertexProtocol {
    /// Distributed propose/partial/aggregate protocol.
    Distributed,
    /// Legacy host-side devnet factory (`L1Driver`).
    #[default]
    DevnetFactory,
}
```

and delete these three fields (with their doc comments and attributes) from `NodeSection`:

```rust
    /// When true, spawn the local L1 certified-vertex driver (plan 06b-L1).
    #[serde(default)]
    pub l1_driver_enabled: bool,
    /// When true, build real BLS quorum certs via `dag::cert` (07a).
    #[serde(default)]
    pub l1_real_vertex_certs: bool,
    /// Which L1 vertex production path runs (06-04 design):
    /// `"distributed"` = propose → partials → 2f+1 CV (production);
    /// `"devnet_factory"` = legacy L1Driver fabrication (default).
    #[serde(default)]
    pub vertex_protocol: VertexProtocol,
```

(`l1_blob_custody_enabled` and everything after it stays.)

- [ ] **Step 2: Clean `devnet.toml`**

In `config/profiles/devnet.toml` delete lines 23-30:

```toml
# Local L1 vertex producer (plan 06b-L1).
l1_driver_enabled = true

# Real BLS vertex quorum certificates (plan 07a).
l1_real_vertex_certs = true

# L1 vertex production: "devnet_factory" (legacy driver) | "distributed" (06-04)
vertex_protocol = "devnet_factory"
```

(The blob custody and erasure blocks that follow stay.)

- [ ] **Step 3: Full workspace test**

Run: `cargo test --workspace --locked`
Expected: PASS across all crates (sim included, untouched).

- [ ] **Step 4: Commit**

```bash
git add apps/node/src/config_layers.rs config/profiles/devnet.toml
git commit -m "feat(node): drop vertex_protocol/l1_driver_enabled/l1_real_vertex_certs config"
```

---

### Task 6: Documentation updates

**Files:**
- Modify: `docs/architecture/layer-1.md` (control-plane subgraph + edges)
- Modify: `docs/superpowers/plans/2026-05-23-06b-l1-vertex-driver.md:3-7` (extend deprecation note)
- Modify: `docs/superpowers/plans/2026-06-11-distributed-vertex-certificate.md:3` (add removal note)

**Interfaces:** none (docs only).

- [ ] **Step 1: Update the architecture diagram**

In `docs/architecture/layer-1.md`, replace the control-plane subgraph:

```
        subgraph ControlPlane["Control Plane (Vertex Certification)"]
            direction TB
            PQ[("Pending Queue<br/>blobs this node submitted<br/>awaiting anchor")]
            L1Driver["L1 Driver / Proposer<br/>drain_pending() each tick<br/>round-robin to 2f+1 authors"]
            CertBuilder["Certificate Protocol<br/>Collect ≥ 2f+1 BLS"]
            LiveDag["LiveDag / Orchestrator<br/>In-memory & DB"]
        end
```

with:

```
        subgraph ControlPlane["Control Plane (Distributed Vertex Certification)"]
            direction TB
            PQ[("Pending Queue<br/>blobs this node submitted<br/>awaiting anchor")]
            Proposer["vertex_cert Proposer<br/>drain_pending() → own proposal<br/>one vertex per validator per round"]
            CertBuilder["Certificate Protocol<br/>proposer aggregates ≥ 2f+1 BLS partials"]
            LiveDag["LiveDag / Orchestrator<br/>In-memory & DB"]
        end
```

and replace the three control-plane edges:

```
    PQ -->|"Vec&lt;BlobRef&gt;"| L1Driver
    L1Driver -->|"broadcast vertex header"| Gossip
    Gossip -->|"BLS partial sigs"| CertBuilder
```

with:

```
    PQ -->|"Vec&lt;BlobRef&gt;"| Proposer
    Proposer -->|"broadcast VertexProposal"| Gossip
    Gossip -->|"VertexPartial (BLS)"| CertBuilder
```

- [ ] **Step 2: Superseded notes on legacy plans**

In `docs/superpowers/plans/2026-05-23-06b-l1-vertex-driver.md`, extend the existing deprecation block (after line 7) with:

```markdown
> **REMOVED (2026-07-06):** `devnet_factory`, `L1Driver`, and the `vertex_protocol` /
> `l1_driver_enabled` / `l1_real_vertex_certs` flags were deleted; distributed is the only
> L1 production path. See
> [`2026-07-06-remove-devnet-factory-design.md`](../specs/2026-07-06-remove-devnet-factory-design.md).
```

In `docs/superpowers/plans/2026-06-11-distributed-vertex-certificate.md`, insert after line 3 (the agentic-workers note):

```markdown
> **UPDATE (2026-07-06):** The `vertex_protocol` rollout flag and the legacy
> `devnet_factory` path this plan kept as default were removed; distributed is now the
> only L1 production path. See
> [`2026-07-06-remove-devnet-factory-design.md`](../specs/2026-07-06-remove-devnet-factory-design.md).
```

- [ ] **Step 3: Commit**

```bash
git add docs/architecture/layer-1.md docs/superpowers/plans/2026-05-23-06b-l1-vertex-driver.md docs/superpowers/plans/2026-06-11-distributed-vertex-certificate.md
git commit -m "docs: layer-1 diagram distributed-only; mark devnet_factory removed"
```

---

### Task 7: Final verification sweep

**Files:** none (verification only).

- [ ] **Step 1: Residual-reference sweep**

Run: `rg "devnet_factory|DevnetFactory|l1_driver_enabled|l1_real_vertex_certs|VertexProtocol" apps crates config`
Expected: no matches. (Hits under `docs/` are fine and expected.)

- [ ] **Step 2: Full workspace test**

Run: `cargo test --workspace --locked`
Expected: PASS.

- [ ] **Step 3 (optional, manual): devnet compose smoke**

If Docker is available: `docker compose up` the 4-node devnet, then check logs for `L1 distributed vertex certification active` on each node and confirm `lua_getCausalSet` returns growing vertex hashes across rounds. This is the first time devnet runs the distributed protocol for real — a liveness stall here is a distributed-path bug surfacing, not a plan error.

- [ ] **Step 4: Done**

Report completion; branch is ready for review/merge per `superpowers:finishing-a-development-branch`.
