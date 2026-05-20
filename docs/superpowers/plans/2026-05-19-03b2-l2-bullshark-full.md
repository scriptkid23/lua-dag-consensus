# L2 Bullshark Full (03b-2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace 03b-1 relaxed L2 with whitepaper §8 Bullshark (ECVRF anchor, shortcut/slow commit, BFS linearization, real MicroQC) and extend `sim` for multi-vertex rounds and stress scenarios.

**Architecture:** Delete `consensus::l2_minimal`; `state_machine` dispatches into `bullshark::*` and `leader::*`. `sim` factory emits enough vertices per round for `2f+1` participation where required. Increase `Actions` capacity if tests show >8 actions per event.

**Tech Stack:** Rust 1.88, `consensus`, `crypto` (ECVRF), `sim`, existing `Config::bullshark` / `Config::leader`.

**Prerequisites:** Plan `2026-05-19-03b1-l2-minimal-sim.md` complete (green `happy_path`).

**Spec:** `docs/superpowers/specs/2026-05-19-l2-sim-milestone-a-design.md` §6

---

## File map

| Area | Files |
|------|--------|
| Remove | `crates/consensus/src/l2_minimal/**` |
| Bullshark | `crates/consensus/src/bullshark/{anchor,commit,linearize,micro_qc}.rs` |
| Leader | `crates/consensus/src/leader/{vrf_sortition,beacon,timeout}.rs` |
| SM | `crates/consensus/src/state_machine.rs` |
| Sim factory | `apps/sim/src/vertex_factory.rs`, `apps/sim/src/world.rs` |
| Sim net | `apps/sim/src/virtual_net.rs` (latency option) |
| Scenarios | `apps/sim/src/scenarios/{anchor_dos,network_partition}.rs` |
| Tests | `crates/consensus/tests/bullshark_*.rs` |

---

### Task 1: Remove `l2_minimal` and restore dispatch skeleton

**Files:**
- Delete: `crates/consensus/src/l2_minimal/`
- Modify: `crates/consensus/src/lib.rs`, `state_machine.rs`
- Modify: `crates/consensus/tests/l2_minimal_happy.rs` → rename/mark `#[ignore]` until rewritten

- [ ] **Step 1: Delete `l2_minimal` module; remove `book` field from `StateMachine`**

Move the `emitted_micro_qc: HashSet<Hash32>` idempotency set from `Book` into a new `crate::bullshark::micro_qc::EmittedSet` field on `StateMachine` (keep `MicroQcAssembled` idempotent across the deletion — `Book`'s only surviving responsibility). All other `Book` state (wave_status, seen) is rebuilt from `DagView`.

- [ ] **Step 2: `step` calls stub `bullshark` handlers that return empty until Tasks 2–5 land**

To keep `cargo test --workspace --locked` green throughout 03b-2 execution:

- Mark `apps/sim/src/scenarios/happy_path.rs` (and the consensus test `bullshark_happy`) `#[ignore = "re-enabled after 03b-2 Task 5"]`.
- Re-enable both in Task 5 Step 5 (remove the `#[ignore]` along with rewriting the test).

This keeps each task's commit boundary green; readers running `cargo test` between tasks should see the ignored count rise and fall, not failures.

- [ ] **Step 3: Commit**

```bash
git commit -m "refactor(consensus): remove l2_minimal ahead of full Bullshark"
```

---

### Task 2: `bullshark/anchor` + `leader/vrf_sortition`

**Files:**
- Modify: `crates/consensus/src/bullshark/anchor.rs`
- Modify: `crates/consensus/src/leader/vrf_sortition.rs`
- Create: `crates/consensus/tests/bullshark_anchor.rs`

- [ ] **Step 1: Failing test — known seed picks expected author**

```rust
#[test]
fn vrf_sortition_is_deterministic_for_seed() {
    let beacon = TestBeacon::new([7u8; 32]);
    let set = fixture_validator_set(4);
    let choice = select_anchor(WaveId(0), &set, &beacon, &cfg.leader).unwrap();
    assert_eq!(choice.author, ValidatorId([/* golden */]));
}
```

- [ ] **Step 2: Implement `select_anchor` using `crypto::vrf` + stake weights**

- [ ] **Step 3: Run `cargo test -p consensus bullshark_anchor --locked` — PASS**

- [ ] **Step 4: Commit**

---

### Task 3: `bullshark/linearize` — BFS closure

**Files:**
- Modify: `crates/consensus/src/bullshark/linearize.rs`
- Create: `crates/consensus/tests/bullshark_linearize.rs`

- [ ] **Step 1: Test fixture DAG in `HashMapDag` (5–8 vertices)**

- [ ] **Step 2: `Linearization::closure_of_anchor(anchor_hash, dag)` returns BFS order**

Use `DagView::vertex` + parent links; cycle-safe visited set.

- [ ] **Step 3: Assert order matches golden permutation for fixture**

- [ ] **Step 4: Commit**

---

### Task 4: `bullshark/commit` — shortcut + slow path

**Files:**
- Modify: `crates/consensus/src/bullshark/commit.rs`
- Modify: `crates/consensus/src/leader/timeout.rs`
- Create: `crates/consensus/tests/bullshark_commit.rs`

- [ ] **Step 1: Test shortcut path — 4 rounds + anchor → `CommitDecision::Shortcut`**

Use shortened `Config { bullshark: { shortcut_round_count: 4, ... }, timing: { round_duration_ms: 1 } }`.

- [ ] **Step 2: Implement shortcut detector per §8 (read vertices via `DagView`)**

- [ ] **Step 3: Test slow path — anchor missing until timer `TimerFired`**

Inject `ScheduleTimer` from SM; sim delivers timer after `slow_path_round_count`.

- [ ] **Step 4: Implement slow path branch**

- [ ] **Step 5: Commit**

---

### Task 5: `bullshark/micro_qc` + wire `state_machine`

**Files:**
- Modify: `crates/consensus/src/bullshark/micro_qc.rs`
- Modify: `crates/consensus/src/state_machine.rs`

- [ ] **Step 1: Replace `try_finalize` skeleton with stake threshold from `ValidatorSetPort`**

- [ ] **Step 2: `CertifiedVertexReceived` path:**

```text
anchor/wave state update → try commit → linearize → micro_qc.try_finalize → BroadcastMicroQc
```

- [ ] **Step 3: Keep `MicroQcAssembled` idempotent**

State home: `bullshark::micro_qc::EmittedSet` on `StateMachine` (created in Task 1 Step 1). On `MicroQcAssembled(qc)`, if `qc.checkpoint_hash` is already in the set, return `Actions::new()` — peer-merge only, no re-broadcast.

- [ ] **Step 4: Count max actions per event in tests; if >8, bump `Actions` type:**

```rust
pub type Actions = SmallVec<[Action; 16]>;
```

- [ ] **Step 5: Rewrite `l2_minimal_happy` → `bullshark_happy` with full rules**

Remove the `#[ignore]` placed on `bullshark_happy` and on `apps/sim/src/scenarios/happy_path.rs` in Task 1 Step 2 in the same commit as the rewrite.

- [ ] **Step 6: `cargo test -p consensus --locked` — PASS**

- [ ] **Step 7: Commit**

---

### Task 6: `sim` multi-vertex factory

**Files:**
- Modify: `apps/sim/src/vertex_factory.rs`
- Modify: `apps/sim/src/world.rs`

- [ ] **Step 1: For each virtual round, emit `2f+1` vertices from distinct validators** (equal stake)

Proposers: `(r + i) % n` for `i in 0..=2f`. Vertex hash recipe must include the **author** (it already does — `vertex_hash(round, &author)` from 03b-1 Task 6), so sibling vertices in the same round produce distinct hashes; no collision risk under the multi-vertex factory.

- [ ] **Step 2: Parents link to prior round smallest hash (same as 03b-1)**

- [ ] **Step 3: Integration test: after one wave, `dag.vertices_at_round` counts meet commit preconditions**

- [ ] **Step 4: Commit**

---

### Task 7: `sim` scenarios + optional net latency

**Files:**
- Modify: `apps/sim/src/scenarios/happy_path.rs`
- Modify: `apps/sim/src/scenarios/anchor_dos.rs`
- Modify: `apps/sim/src/scenarios/network_partition.rs`
- Modify: `apps/sim/src/virtual_net.rs`

- [ ] **Step 1: `happy_path` green under full Bullshark**

```bash
cargo test -p sim happy_path --locked
```

- [ ] **Step 2: `anchor_dos` — withhold anchor vertex from subset; assert `liveness_ok: false` or documented partial progress**

Remove `#[ignore]` only when stable; else `#[ignore = "flaky on CI"]` + issue link in report `notes`.

- [ ] **Step 3: `network_partition` — split `VirtualNet` delivery by partition; heal and assert recovery**

- [ ] **Step 4: Optional: `VirtualNet` delay from `Config` with seeded jitter**

```rust
deliver_at: now + base_delay + (rng.gen::<u64>() % jitter_ns)
```

- [ ] **Step 5: Commit**

---

### Task 8: Workspace + regression

- [ ] **Step 1:**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

- [ ] **Step 2: Document golden seed regeneration in `consensus/tests/README` or plan footer**

Seeds: `happy_path` seed 42, anchor golden in `bullshark_anchor.rs`.

- [ ] **Step 3: Final commit if needed**

```bash
git commit -m "feat(consensus): full Bullshark L2 on sim"
```

---

## Plan self-review

| Spec §6 | Task |
|---------|------|
| Replace 03b-1 code | Task 1 |
| anchor ECVRF | Task 2 |
| linearize BFS | Task 3 |
| commit shortcut/slow | Task 4 |
| micro_qc | Task 5 |
| sim multi-vertex | Task 6 |
| stress scenarios | Task 7 |
| Actions cap | Task 5 step 4 |
| Acceptance §6.4 | Task 8 |

---

## Execution handoff

Plan saved to `docs/superpowers/plans/2026-05-19-03b2-l2-bullshark-full.md`.

Execute **after** 03b-1 is green. Same execution options as 03b-1 plan.
