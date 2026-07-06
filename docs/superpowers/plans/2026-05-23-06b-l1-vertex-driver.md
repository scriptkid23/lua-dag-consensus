# L1 Certified-Vertex Driver (06b-L1 remainder) Implementation Plan

> **DEPRECATION (2026-06-11):** The centralized `L1Driver` produce path this plan built is
> now the legacy `vertex_protocol = "devnet_factory"` mode. The production path is the
> distributed protocol from
> [`2026-06-04-distributed-vertex-certificate-design.md`](../specs/2026-06-04-distributed-vertex-certificate-design.md),
> implemented by [`2026-06-11-distributed-vertex-certificate.md`](2026-06-11-distributed-vertex-certificate.md).

> **REMOVED (2026-07-06):** `devnet_factory`, `L1Driver`, and the `vertex_protocol` /
> `l1_driver_enabled` / `l1_real_vertex_certs` flags were deleted; distributed is the only
> L1 production path. See
> [`2026-07-06-remove-devnet-factory-design.md`](../specs/2026-07-06-remove-devnet-factory-design.md).

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `apps/node` self-drive L2 Bullshark without an external vertex feed: on each micro-round tick, produce a quorum of `CertifiedVertex` values (mirroring `apps/sim`), persist them via `LiveDag`, inject `Event::CertifiedVertexReceived` into the orchestrator, and **publish** them on gossip topic `certified-vertex` so peer nodes can ingest and run the same SM path.

**Architecture:** Vertex production is **host responsibility** (same as `sim::World::produce_vertex_tick`), not an SM `Action`. Add a dedicated **`L1Driver`** tokio task in `apps/node` that ticks on `config.timing.round_duration_ms`, builds vertices from the loaded validator set, calls `LiveDag::ingest`, pushes events to `events_tx`, and publishes Borsh payloads on `Topic::CertifiedVertex` through a small net helper (parallel to swarm outbound, without extending `Action` enum). The existing orchestrator path (ingress → `LiveDag::ingest` → `sm.step`) stays unchanged for gossip-received vertices.

**Tech Stack:** Rust 1.88, `tokio`, `consensus`, `net`, `storage`, `apps/node`, `types`.

**Spec:** [`docs/superpowers/specs/2026-05-22-l3-macro-finality-design.md`](../specs/2026-05-22-l3-macro-finality-design.md) §4 follow-on **06b-L1**; [`docs/superpowers/specs/2026-05-19-l2-sim-milestone-a-design.md`](../specs/2026-05-19-l2-sim-milestone-a-design.md) §5.5 tick order.

**Prerequisite:** **06b-l3** complete (`LiveDag`, gossip L3, `ActionApplier`, valset TOML, `l3_wire_complete = true`). Ingress half of 06b-L1 already landed (`orchestrator` ingests `CertifiedVertexReceived`; `gossip_wire` decodes `Topic::CertifiedVertex`).

---

## Current gap (why this plan exists)

| Area | Today (post-06b-L1 ingress) | Target |
|------|----------------------------|--------|
| Vertex source on node | Only **inbound gossip** | **Local tick producer** + gossip publish |
| L2 progress | SM idle unless peers gossip vertices | Bullshark commits waves locally on schedule |
| L2→L3 pipeline on node | Blocked without external feed | `CertifiedVertex` → `BroadcastMicroQc` → macro path (same as sim) |
| Gossip outbound for L1 | `outbound_broadcast` has **no** `CertifiedVertex` arm | Publish helper for `Topic::CertifiedVertex` |
| Devnet 4-node | Nodes gossip L3 artifacts but may not advance L2 together | All nodes produce compatible quorum vertices |

**Already landed (do not redo):**

- `apps/node/src/live_dag.rs` — `ingest` + `DagView` + `vertex_store::put`
- `apps/node/src/orchestrator.rs` — pre-step `LiveDag::ingest` on `CertifiedVertexReceived`
- `crates/net/src/gossip_wire.rs` — inbound `Topic::CertifiedVertex` → `Event::CertifiedVertexReceived`
- `crates/net/src/gossip/topics.rs` — `Topic::CertifiedVertex` wire name registered

---

## Design decisions (lock-in)

| Topic | Decision |
|-------|----------|
| SM `Action` enum | **No** `BroadcastCertifiedVertex` — L1 feed is host-side like sim, not SM output |
| Vertex builder | Phase 1: **devnet fixture certificates** (same as `sim::vertex_factory::fixture_certificate`) — Bullshark does not verify vertex certs today |
| Validator ids | Use **`ValidatorId`s from loaded valset TOML** (`devnet-4.toml` / `devnet_keys`), not sim's `validator_id_for_index` |
| Quorum size | `2f+1` vertices per virtual round (`quorum_vertex_count(n)`) — copy formula from sim |
| Parent link | `parent_hash_for_round(r)` = lexicographically smallest vertex hash at round `r-1` (mirror `sim::World`) |
| Tick order | Match sim §5.5: **drain net → produce vertices → drain timers → advance clock** (orchestrator handles net; driver handles produce + clock advance for virtual round counter) |
| Dedup | `LiveDag::ingest` is idempotent by hash; re-gossip of own vertex may re-enter SM — acceptable in phase 1 (Bullshark `EmittedSet` prevents duplicate MicroQc broadcast) |
| Config gate | `[node].l1_driver_enabled = true` in devnet profile; `false` preserves ingress-only mode |
| Real vertex BLS certs | **Out of scope** — follow-up after L1 availability DAG lands |

---

## File map

| File | Action |
|------|--------|
| `crates/types/src/dag/` or `apps/node/src/l1/` | **CREATE** shared vertex builder (or `apps/node/src/l1/vertex_builder.rs`) |
| `apps/node/src/l1/driver.rs` | **CREATE** `L1Driver` tick loop |
| `apps/node/src/l1/mod.rs` | **CREATE** module root |
| `apps/node/src/lib.rs` | export `l1` module |
| `apps/node/src/config_layers.rs` | add `l1_driver_enabled: bool` to `NodeSection` |
| `config/profiles/devnet.toml` | set `l1_driver_enabled = true` |
| `apps/node/src/runtime.rs` | spawn `L1Driver` when enabled; share `events_tx`, `LiveDag`, valset |
| `crates/net/src/gossip_wire.rs` | **ADD** `publish_certified_vertex(cv) -> Result<(Topic, Vec<u8>)>` helper |
| `crates/net/src/swarm_runner.rs` | expose publish channel OR accept pre-encoded publish tx from driver |
| `apps/node/tests/l1_driver_smoke.rs` | **CREATE** single-node: N ticks → ≥1 MicroQc persisted |
| `apps/node/tests/l1_gossip_roundtrip.rs` | **CREATE** two-node: driver on A, B receives vertex via gossip |
| `docs/superpowers/specs/2026-05-22-l3-macro-finality-design.md` | status bump for 06b-L1 complete |

---

### Task 1: Vertex builder (devnet quorum)

**Files:**
- Create: `apps/node/src/l1/vertex_builder.rs`
- Modify: `apps/node/src/l1/mod.rs`, `apps/node/src/lib.rs`

- [ ] **Step 1: Failing unit test**

```rust
#[test]
fn builds_quorum_for_devnet_four() {
    let valset = devnet_keys::build_devnet_valset_4(); // or load fixture
    let batch = build_quorum_vertices_for_valset(0, &valset, None);
    assert_eq!(batch.len(), 3); // 2f+1 for n=4
    assert!(batch.iter().all(|v| valset.entries.iter().any(|e| e.id == v.vertex.author)));
}
```

- [ ] **Step 2: Implement**

```rust
/// Deterministic vertex hash: BLAKE3(SIM_VERTEX_HASH, round || author).
pub fn vertex_hash(round: u64, author: &ValidatorId) -> Hash32 { /* ... */ }

pub fn build_certified_vertex(
    round: u64,
    author: ValidatorId,
    parent: Option<Hash32>,
) -> CertifiedVertex { /* fixture BlsAggSig like sim */ }

pub fn build_quorum_vertices_for_valset(
    round: u64,
    valset: &ValidatorSet,
    parent: Option<Hash32>,
) -> Vec<CertifiedVertex> {
    let n = valset.entries.len() as u32;
    let quorum = 2 * ((n - 1) / 3) + 1;
    (0..quorum).map(|i| {
        let idx = ((round + i as u64) % n as u64) as usize;
        let author = valset.entries[idx].id;
        build_certified_vertex(round, author, parent)
    }).collect()
}
```

- [ ] **Step 3: Run**

```bash
cargo test -p node vertex_builder --locked
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(node): devnet quorum certified-vertex builder (06b-L1)"
```

---

### Task 2: Gossip publish helper for `CertifiedVertex`

**Files:**
- Modify: `crates/net/src/gossip_wire.rs`
- Modify: `crates/net/src/lib.rs` (re-export if needed)

- [ ] **Step 1: Failing test** in `crates/net/tests/gossip_roundtrip.rs` or new test:

```rust
#[test]
fn certified_vertex_encode_for_publish() {
    let cv = /* minimal CertifiedVertex */;
    let (topic, bytes) = gossip_wire::encode_certified_vertex(&cv).unwrap();
    assert_eq!(topic, Topic::CertifiedVertex);
    let ev = gossip_wire::inbound_message(&topic.wire_name(), &bytes).unwrap();
    assert!(matches!(ev, Some(Event::CertifiedVertexReceived(g)) if g == cv));
}
```

- [ ] **Step 2: Implement**

```rust
pub fn encode_certified_vertex(cv: &CertifiedVertex) -> Result<(Topic, Vec<u8>)> {
    Ok((Topic::CertifiedVertex, encode_action_payload(cv)?))
}
```

- [ ] **Step 3: Wire publish path in swarm**

Option A (preferred): add `mpsc::Sender<(Topic, Vec<u8>)>` **`publish_tx`** fed into `swarm_runner` alongside `actions_rx`; driver sends pre-encoded pairs; swarm task calls `swarm.behaviour_mut().publish`.

Option B: extend `outbound_broadcast` with a synthetic internal path — avoid coupling to `Action`.

- [ ] **Step 4: Run**

```bash
cargo test -p net gossip --locked
```

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(net): gossip publish helper for CertifiedVertex (06b-L1)"
```

---

### Task 3: `L1Driver` tick loop

**Files:**
- Create: `apps/node/src/l1/driver.rs`

- [ ] **Step 1: Struct + spawn**

```rust
pub struct L1Driver {
    virtual_round: u64,
    valset: ValidatorSet,
    dag: Arc<LiveDag>,
    events_tx: mpsc::Sender<Event>,
    publish_tx: mpsc::Sender<(Topic, Vec<u8>)>,
    round_duration: Duration,
}

impl L1Driver {
    pub async fn run(mut self) {
        let mut interval = tokio::time::interval(self.round_duration);
        loop {
            interval.tick().await;
            self.tick_round().await;
        }
    }

    async fn tick_round(&mut self) {
        let parent = self.parent_hash_for_round(self.virtual_round);
        let batch = build_quorum_vertices_for_valset(self.virtual_round, &self.valset, parent);
        for cv in batch {
            if let Err(e) = self.dag.ingest(cv.clone()) {
                tracing::warn!(%e, "l1 driver ingest failed");
                continue;
            }
            let (topic, bytes) = match gossip_wire::encode_certified_vertex(&cv) {
                Ok(x) => x,
                Err(e) => { tracing::warn!(%e, "encode vertex"); continue; }
            };
            let _ = self.publish_tx.try_send((topic, bytes));
            let _ = self.events_tx.try_send(Event::CertifiedVertexReceived(cv)).await;
        }
        self.virtual_round += 1;
    }
}
```

- [ ] **Step 2: Parent hash helper** — copy logic from `sim::World::parent_hash_for_round`.

- [ ] **Step 3: Unit test with mock channels** (no network): assert `events_tx` receives `2f+1` events per tick.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(node): L1Driver micro-round tick (06b-L1)"
```

---

### Task 4: Runtime wiring + config flag

**Files:**
- Modify: `apps/node/src/config_layers.rs`, `config/profiles/devnet.toml`
- Modify: `apps/node/src/runtime.rs`, `crates/net/src/swarm_runner.rs`

- [ ] **Step 1: Config**

```toml
# config/profiles/devnet.toml
[node]
l1_driver_enabled = true
```

```rust
// NodeSection
#[serde(default)]
pub l1_driver_enabled: bool,
```

- [ ] **Step 2: Spawn in `run_async`**

After swarm + orchestrator are wired:

```rust
if cfg.node.l1_driver_enabled {
    let driver = L1Driver::new(
        valset.clone(),
        Arc::clone(&live_dag),
        events_tx.clone(),
        publish_tx, // from swarm spawn
        Duration::from_millis(cfg.consensus.timing.round_duration_ms),
    );
    tokio::spawn(async move { driver.run().await });
}
```

- [ ] **Step 3: Ensure tick does not fight orchestrator clock** — `L1Driver` owns `virtual_round`; `TokioClock` remains wall-clock for timers SM schedules.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(node): spawn L1Driver from devnet profile (06b-L1)"
```

---

### Task 5: Integration tests

**Files:**
- Create: `apps/node/tests/l1_driver_smoke.rs`
- Create: `apps/node/tests/l1_gossip_roundtrip.rs`

- [ ] **Step 1: Single-node smoke**

1. Temp data dir, open RocksDB, build orchestrator + driver with `l1_driver_enabled`.
2. Run 16 virtual rounds (~4 waves).
3. Assert `RocksPersistence::micro_qc_for` or scan finds ≥1 MicroQc OR metrics `actions_dispatched` > 0 with BroadcastMicroQc.

- [ ] **Step 2: Two-node gossip** (pattern from `l3_gossip_smoke.rs`)

1. Node A runs driver; Node B ingress-only.
2. Within timeout, B's orchestrator processes `CertifiedVertexReceived` from gossip (metric or persistence).

- [ ] **Step 3: Run**

```bash
cargo test -p node l1_ --locked
```

- [ ] **Step 4: Commit**

```bash
git commit -m "test(node): L1 driver smoke + gossip roundtrip (06b-L1)"
```

---

### Task 6: Acceptance + docs

- [ ] **Step 1: Regression**

```bash
cargo test -p consensus -p net -p sim -p node --locked
```

- [ ] **Step 2: Manual devnet sanity**

```bash
docker compose up --build -d
# watch logs for BroadcastMicroQc / macro activity on node0
docker compose logs -f node0 | findstr /i "micro macro vertex"
```

- [ ] **Step 3: Update spec** — `2026-05-22-l3-macro-finality-design.md` §4: mark **06b-L1** landed (driver + ingress).

- [ ] **Step 4: Commit**

```bash
git commit -m "docs: mark 06b-L1 vertex driver landed"
```

---

## Done — 06b-L1 acceptance criteria

- With `l1_driver_enabled = true`, a single node produces quorum vertices every `round_duration_ms` without external input.
- Vertices are persisted in RocksDB `vertex` CF and visible via `LiveDag` / `DagView`.
- SM emits L2 actions (`BroadcastMicroQc`, timers) and downstream L3 actions follow in devnet config (same as sim happy-path shape).
- Peers receive vertices on gossip topic `certified-vertex` and run the existing ingress path.
- With `l1_driver_enabled = false`, behavior unchanged (ingress-only).

**Non-goals (explicit):**

- Real BLS vertex certificate aggregation / verification
- L1 blob payload / availability proofs
- Checkpoint sync for late joiners

**Next:** [BlobStatus persist](./2026-05-23-blob-status-persist.md) → [Devnet E2E smoke](./2026-05-15-devnet-prodlike.md) Task 13 acceptance.
