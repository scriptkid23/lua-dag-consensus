# LiveDag — High-Traffic Scaling Guide

Runbook for scaling `LiveDag` when gossip ingest, consensus reads, or RPC
queries start to contend on the in-memory DAG indexes.

**Primary code:** `apps/node/src/live_dag.rs`  
**Trait:** `crates/consensus/src/ports/dag_view.rs`  
**Related:** `apps/node/src/orchestrator.rs`, `crates/consensus/src/bullshark/`

---

## 1. Current design (baseline)

```rust
pub struct LiveDag {
    by_hash: RwLock<HashMap<Hash32, SharedCertifiedVertex>>,   // Arc<CertifiedVertex>
    by_round: RwLock<HashMap<Round, Vec<SharedCertifiedVertex>>>,
    db: Arc<Database>,
}
```

| Piece | Role |
|---|---|
| `SharedCertifiedVertex` | `Arc<CertifiedVertex>` — one heap allocation, many cheap pointer clones |
| `by_hash` | O(1) point lookup by content hash (BFS parents, Bullshark L2) |
| `by_round` | O(1) lookup all vertices in a round (quorum, anchor, RPC range) |
| `db` | RocksDB `vertex` CF — durable store; RAM indexes are the hot path |

### Ingest flow

```
CertifiedVertex (from gossip / local cert loopback)
    → verify_certified_vertex (orchestrator)
    → ingest(v)
        1. vertex_store::put (RocksDB)
        2. shared = Arc::new(v)
        3. by_hash.insert(hash, Arc::clone(&shared))
        4. by_round[round].push(shared)   // same Arc, not a second copy
```

### Query flow

```
DagView::vertex(hash)           → clone Arc (8 bytes)
DagView::vertices_at_round(r)   → clone Vec<Arc>
Bullshark BFS / commit          → many Arc clones, no deep struct copy
```

**Already done (2026-07):** Arc sharing removed the main bottleneck (deep-cloning
`CertifiedVertex` on every read and storing two full copies in RAM).

**Remaining bottleneck under high traffic:** `RwLock` global exclusion on each
map — one writer blocks all readers on that map.

---

## 2. When `RwLock<HashMap>` becomes a problem

### Semantics (one lock per map)

| State | Readers | Writers |
|---|---|---|
| No lock held | OK | OK |
| One or more readers | More readers OK | Writer **waits** |
| One writer | **All readers wait** | Other writers wait |

Important: a write to hash `A` blocks a read of hash `Z` on the **same**
`RwLock<HashMap>`. The lock is on the **whole container**, not per key.

`by_hash` and `by_round` use **separate** locks — they do not block each other.

### Symptoms (investigate when you see these)

- `vertex()` or `vertices_at_round()` p99 spikes during gossip catch-up / bursts
- Consensus round latency correlates with ingest rate
- RPC `causal_set` slow only while nodes are syncing certified vertices
- CPU rises but ingest throughput does not scale with more gossip workers
- Single ingest task is fine; problem appears only under concurrent read + write

### When it is **not** worth changing yet

- Devnet 4 validators, low write rate
- Lock wait &lt; ~1% of lookup/ingest wall time (heuristic)
- RocksDB `put` dominates ingest latency (&gt;80–90%)
- Only **one** task calls `ingest()` (no parallel writers — DashMap helps less)

---

## 3. Optimization roadmap (by ROI)

Apply in this order. Do not skip straight to DashMap without metrics.

### Phase 0 — Instrument (before any struct change)

Add sampled metrics (e.g. 1/1024 ops) for:

| Metric | Meaning |
|---|---|
| `by_hash_read_lock_wait` | Time waiting to acquire read lock |
| `by_hash_write_lock_wait` | Time waiting to acquire write lock |
| `by_hash_read_lock_hold` | Time holding read lock |
| `by_hash_write_lock_hold` | Time holding write lock |
| `vertex_lookup_latency` p50/p95/p99 | End-to-end `DagView::vertex` |
| `ingest_latency` p50/p95/p99 | RocksDB + RAM index |
| `causal_set_rpc_latency` | RPC path using `certified_hashes_in_range` |

Also track: live vertex count, rounds retained in RAM, gossip ingest rate.

### Phase 1 — High ROI, low risk

1. **Bounded live window + pruning**  
   Drop old rounds from RAM; keep history in RocksDB. Prune order:
   - Remove from `by_round` first
   - Then remove hashes from `by_hash`  
   Insert order (already correct): `by_hash` → `by_round`.

2. **Shrink lock scope on range reads**  
   In `certified_hashes_in_range`, clone `Arc`s inside a short read guard,
   then release lock before building the RPC response.

3. **Batch ingest** (if multiple vertices arrive together)  
   - RocksDB `WriteBatch` for all puts  
   - One `by_hash` write lock for the batch  
   - One `by_round` write lock for the batch  

4. **`Event::CertifiedVertexReceived` → `Arc<CertifiedVertex>`** (optional)  
   Removes the extra `cv.clone()` in orchestrator before `ingest()`.  
   Smaller win than Phase 1 items; do if profiling shows Event clone matters.

### Phase 2 — `by_round` structure (when range / retention matters)

Do **not** use DashMap for `by_round` (hot single round, range scan).

| Condition | Structure |
|---|---|
| Rounds mostly contiguous + fixed retention window | `RwLock<VecDeque<RoundBucket>>` + `base_round` |
| Sparse rounds, ordered range queries, pruning by interval | `RwLock<BTreeMap<Round, Vec<Arc<...>>>>` |
| Short ranges, devnet-scale | Keep `HashMap` + `for round in start..=end` |

### Phase 3 — `by_hash` → DashMap (only if metrics justify)

See [Section 4](#4-dashmap-migration-by_hash).

### Phase 4 — Micro-optimizations (last)

- `FxHashMap` / custom hasher for `Hash32` — benchmark only; mind HashDoS threat model
- `scc::HashMap` / `scc::HashIndex` — only if DashMap still contends after Phase 3
- Manual sharding — if you want stricter API control than DashMap guards

---

## 4. DashMap migration (`by_hash`)

### Why DashMap helps

DashMap ≈ `N` shards, each `RwLock<HashMap<...>>`. Write to hash in shard 3 only
blocks reads/writes on shard 3, not the whole map.

Default shard count ≈ `next_power_of_two(available_parallelism × 4)`.

### Why **not** to migrate blindly

- After `Arc`, read critical section is already tiny (lookup + atomic inc).
- DashMap adds per-lookup shard routing overhead — can be slower **without** contention.
- `get()` returns a **guard** (`Ref`) — easy to misuse (deadlock, hold across `.await`).
- Does not speed up RocksDB or single-threaded ingest.

### Decision thresholds (heuristics)

**Consider DashMap** when production-like benchmarks show:

- `by_hash` lock wait &gt; ~3–5% of lookup/ingest wall time, **or**
- `vertex()` p99 under gossip burst &gt; ~10–20% above uncontended baseline, **and**
- DashMap improves an **end-to-end** metric by ≥ ~10% (BFS p99, causal_set RPC,
  ingest throughput, catch-up duration).

**Stay on `RwLock<HashMap>`** when:

- Lock wait &lt; ~1%
- Only one ingest writer
- RocksDB dominates ingest latency
- DashMap wins microbenchmarks but not end-to-end

### Safe wrapper pattern (required)

Never expose `DashMap::Ref` to callers. Always clone `Arc` and drop the guard:

```rust
use dashmap::DashMap;
use types::dag::SharedCertifiedVertex;

pub struct VertexHashIndex {
    inner: DashMap<Hash32, SharedCertifiedVertex>,
}

impl VertexHashIndex {
    pub fn get(&self, hash: &Hash32) -> Option<SharedCertifiedVertex> {
        self.inner
            .get(hash)
            .map(|entry| SharedCertifiedVertex::clone(entry.value()))
    }

    pub fn insert(
        &self,
        hash: Hash32,
        vertex: SharedCertifiedVertex,
    ) -> Option<SharedCertifiedVertex> {
        self.inner.insert(hash, vertex)
    }
}
```

### Pitfalls

| Do not | Why |
|---|---|
| Hold `Ref` across `.await` | Shard read lock held during I/O |
| `get()` then `insert()` on same shard | Self-deadlock |
| Hold `by_hash` guard while locking `by_round` (and reverse elsewhere) | Cross-index deadlock |
| Use `iter()` for consensus-critical logic | Not a point-in-time snapshot |

### Cargo dependency

```toml
# apps/node/Cargo.toml (when implementing)
dashmap = "6"
```

Update `LiveDag.by_hash` type and `DagView` impl in `live_dag.rs`; keep
`by_round` unchanged unless Phase 2 applies.

---

## 5. Do not merge the two indexes

| Index | Query class |
|---|---|
| `by_hash` | Random access by `Hash32` |
| `by_round` | All vertices in round `r`, range scan |

A single index forces one access pattern to scan — bad for BFS or round queries.

**Do** share object ownership: one `Arc` inserted into both indexes (current design).

---

## 6. Benchmark checklist

Compare at minimum:

| Impl | Notes |
|---|---|
| A | `RwLock<HashMap>` (current) |
| B | `DashMap` default shards |
| C | `DashMap` with 16 / 32 / 64 shards |
| D | Manual 32-shard `RwLock<HashMap>` |

### Workloads

| Name | Read | Write |
|---|---|---|
| Consensus stable | 100% | 0% |
| Mixed normal | 95% | 5% |
| Gossip burst | 80% | 20% |
| Catch-up | 50% | 50% |

### Map sizes

Benchmark at expected retention: e.g. 10k / 100k / 1M vertices  
(≈ `validators × vertices_per_round × retained_rounds`).

### Burst test

10s steady → 1s write storm (10× rate) → 10s steady. Measure p99 recovery.

### Pass criteria for DashMap

- Repeatable improvement on **end-to-end** latency or throughput
- RSS increase acceptable (e.g. &lt; ~5–10% LiveDag memory)
- Wrapper prevents guard leaks in code review

---

## 7. Quick reference

```
Traffic low (devnet)     → Keep RwLock<HashMap> + Arc ✅

Traffic rising           → Phase 0 metrics → Phase 1 pruning/batch

by_round slow / RAM grow → VecDeque window or BTreeMap (not DashMap)

by_hash lock contention  → DashMap behind VertexHashIndex wrapper

DashMap still not enough → Benchmark scc::HashIndex or manual sharding
```

---

## 8. References

- Layer diagram (L1 → L2): `docs/architecture/layer-1.md`
- `LiveDag` implementation: `apps/node/src/live_dag.rs`
- `DagView` trait: `crates/consensus/src/ports/dag_view.rs`
- Bullshark read path: `crates/consensus/src/bullshark/commit.rs`
- Arc refactor test: `ingest_shares_one_allocation_across_indexes` in `live_dag.rs`

---

*Last updated: 2026-07-10 — reflects `SharedCertifiedVertex` (Arc) rollout.*
