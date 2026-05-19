# Design: Milestone A — L2 Bullshark on `sim` (phased 03b-1 / 03b-2)

**Date:** 2026-05-19  
**Status:** Revised (post spec review)  
**Audience:** Contributors implementing consensus + sim driver  
**Relations:** Extends `docs/superpowers/specs/2026-05-11-folder-architecture-design.md`, plan `2026-05-12-03-consensus-crate.md` (skeleton). Does **not** include L3 macro (03c), slashing full (03d), or `node` production wire (06b) — those follow Milestone A.

---

## 1. Goals

- Replace the skeleton `StateMachine::step` (always empty `Actions`) with **real L2 behavior** verifiable in **`apps/sim`** before relying on libp2p or Docker.
- Keep **`consensus` pure**: no tokio, libp2p, or RocksDB inside the crate; side effects remain `Action` values (host applies persistence / network).
- Deliver Milestone A in **two phases**:
  - **03b-1 (minimal vertical slice):** honest validators, simplified commit + anchor, end-to-end `CertifiedVertexReceived` → `BroadcastMicroQc` in sim.
  - **03b-2 (Bullshark full):** whitepaper Chapter 8 — waves, ECVRF anchor, shortcut + slow path, BFS linearization, full MicroQC rules.

**Non-goals (Milestone A):**

- L3 macro-finality, adaptive aggregation Modes A/B production paths, slashing detectors (03c / 03d).
- `crates/dag/` L1 production crate; `CertifiedVertex` is supplied by **`sim::VirtualDag`** and events only.
- `apps/node` timer/persist/genesis wire (plan **06b**, after 03b-1 proves SM).
- Mainnet keys, prod profiles, rate limits, HA ops.

---

## 2. Context (current repo)

| Layer | State |
|-------|--------|
| `apps/node` | Orchestrator + gossip plumbing live; SM returns no actions. |
| `crates/net` | Inbound/outbound gossip wire works; not consensus logic. |
| `crates/consensus` | Event/Action/ports/config skeleton; `bullshark/*` stubs. |
| `apps/sim` | `World` has `VirtualDag`, `VirtualNet`, `VirtualPersistence`, … but `sm.step(event)` ignores ports; `VirtualNet::enqueue_from_action` no-op; checkers trivially pass. |
| `apps/cli` | `replay_log` calls `sm.step(ev)` without ports — must migrate with API change. |

---

## 3. Architectural decision: `HostContext` on `step`

**Chosen approach (option 1):** extend the public API to:

```text
StateMachine::step(&mut self, event: Event, ctx: &HostContext<'_>) -> Result<Actions>
```

`HostContext` holds shared references to the five ports (read-only for the duration of `step`):

| Field | Trait | Use in L2 |
|-------|--------|-----------|
| `dag` | `DagView` | Resolve parents, closure, vertices at round |
| `clock` | `Clock` | Schedule timer delays (via `Action::ScheduleTimer`) |
| `valset` | `ValidatorSetPort` | Stake thresholds, validator identity |
| `beacon` | `RandomnessBeacon` | 03b-1: fixed bytes; 03b-2: ECVRF anchor input |
| `persistence` | `Persistence` | Read micro/macro artifacts; SM does **not** call `store_*` directly in 03b-1 |

**Port extension (03b-1, required):** add to `consensus::ports::Persistence`:

```text
fn micro_qc_for(&self, checkpoint_hash: &Hash32) -> Result<Option<MicroQc>>;
```

Implement on `storage::RocksPersistence` (read path) and `sim::VirtualPersistence`. Checkers and tests use this instead of reaching into private maps.

**Rationale:** Matches folder-architecture DI seams; `sim::World` already owns all implementations; `node` can build a `HostContext` from Rocks + Tokio later without changing algorithm code.

**Migration (breaking `step` signature):** update in one workspace PR sequence:

1. `crates/consensus` — `HostContext`, `Persistence::micro_qc_for`, `step` signature.
2. `apps/sim` — driver, checkers, scenarios.
3. `apps/node` — orchestrator passes context (see §8).
4. `apps/cli` — `replay_log` uses a **documented stub `HostContext`** (empty `DagView`, noop persistence) so replay remains “best-effort skeleton replay”; optional follow-up: replay bundles port snapshots.
5. `consensus` / workspace tests.

**Rejected:**

- **Ports inside `StateMachine` as `Box<dyn …>`** — harder deterministic testing and cloning.
- **External driver bypassing `step`** — fragments Event/Action pattern.

---

## 4. Plan file decomposition

Implementation plans (checkbox task style, per superpowers) are **split**:

| ID | Path | Depends on |
|----|------|------------|
| **03b-1** | `docs/superpowers/plans/2026-05-19-03b1-l2-minimal-sim.md` | Plan 03 skeleton |
| **03b-2** | `docs/superpowers/plans/2026-05-19-03b2-l2-bullshark-full.md` | 03b-1 complete |
| **03e** | *(sections inside 03b-1)* — sim driver + checkers | 03b-1 SM API |

Follow-up (outside Milestone A scope doc, listed for ordering):

| ID | Topic |
|----|--------|
| **03c** | L3 macro-finality on sim |
| **03d** | Slashing |
| **06b** | `node`: timer dispatcher, `PersistMacroQc` → `RocksPersistence`, validator set boot |

---

## 5. Phase 03b-1 — L2 minimal vertical slice

### 5.1 Purpose

Prove the **Event → step → Action → sim host → network → Event** loop with real state mutations and at least one `BroadcastMicroQc`, without full Bullshark complexity.

### 5.2 Algorithm simplifications (explicit)

Let `n` = validator count, `f = (n - 1) / 3` (integer division).

| Whitepaper feature | 03b-1 behavior |
|--------------------|----------------|
| Anchor selection | **Round-robin:** unique proposer index `p = round.0 % n` authors the anchor attempt for that round. |
| Commit rule | **Relaxed single path (03b-1 only):** commit wave `w` when (a) the anchor vertex for wave `w` is in `DagView`, and (b) each of the four rounds `4w … 4w+3` has **at least one** certified vertex in `DagView`. **No** per-round `2f+1` distinct authors requirement in 03b-1 (that arrives in 03b-2). No slow-path timeout. |
| Linearization | **Deterministic tie-break:** sort committed vertices by `(round, author)` — documented stand-in for closure BFS until 03b-2. |
| MicroQC | **Mode 0 flat:** when wave `w` commits, derive `checkpoint_hash`, and local view agrees stake in batch ≥ `2f+1` (sim: equal stake → count validators with vertices in wave ≥ `2f+1`), emit **`BroadcastMicroQc` once per wave per validator** (guard in `Book` so no duplicate). |
| `Event::MicroQcAssembled` | **Idempotent merge:** update `Book` with peer QC; if checkpoint already finalized locally, return **empty** `Actions` (no second broadcast). |
| ECVRF / Shoal reputation | Not used; `RandomnessBeacon` returns fixed bytes. |
| L3 events (`Macro*`, macro-path aggregates) | `step` → empty `Actions`, no error. |
| Slashing | No detection; `SlashEvidenceFound` → empty. |
| L1 certificate verify | **Skipped in 03b-1** — sim factory uses deterministic fixture `BlsAggSig`; no crypto verify in `step`. |

### 5.3 `StateMachine` internal state (03b-1)

Add a private `Book` struct inside `consensus` (in-memory only):

- Seen certified vertices (hash → round, author).
- Per-wave status: `Pending` / `Committed`.
- Last committed wave id.
- Per-checkpoint: whether this validator already emitted `BroadcastMicroQc` for that hash.
- Checkpoint hash for last assembled MicroQC (derived from linearized batch).

All mutations in `step`; no hidden globals.

### 5.4 Certified vertex factory invariants (sim)

Each virtual round `r` (monotonic counter in `World`, distinct from Bullshark `Round` inside vertex — factory sets `vertex.round = Round(r)`):

1. Proposer index `p = r % n`; `author = ValidatorId` for index `p`.
2. `parents`: empty at `r == 0`, else `[hash of any vertex from round r-1]` (deterministic: lexicographically smallest hash in `VirtualDag` at `r-1`).
3. `hash`: deterministic function of `(r, author)` (e.g. BLAKE3 with test DST — implementation in plan 03b-1).
4. `certificate`: fixed non-zero `BlsAggSig` + bitmap (not verified in 03b-1).
5. `VirtualDag::insert` then deliver **`CertifiedVertexReceived` synchronously to every validator** before draining `VirtualNet` in the same tick (binding for determinism).

Honest factory produces **one vertex per round**, which is enough for the relaxed commit rule in §5.2.

### 5.5 `sim` driver changes (03b-1)

**`World::tick_round` order (replaces current skeleton order — not backward compatible):**

1. Let `now = clock.now_nanos()`.
2. **Drain `VirtualNet`** for messages with `deliver_at <= now` → for each: `step(event, ctx)` → apply actions (steps 4–5 below).
3. **Produce vertices** for this tick (§5.4) → synchronous `CertifiedVertexReceived` to all validators → `step` + apply actions.
4. **Fire due timers** (`deliver_at <= now`) → `TimerFired` → `step` + apply actions.
5. **Advance clock** by `config.timing.round_duration_ms` (moved to end of tick).

**Per-validator `HostContext`:** shared `dag`, `clock`, `valset`, `beacon`; **per-validator** `persistence`.

**Apply actions (host — not inside `consensus`):**

| `Action` | Host behavior |
|----------|----------------|
| `BroadcastMicroQc(qc)` | `VirtualNet::enqueue_from_action` **and** `persistence.store_micro_qc(qc)` on the **sender's** `VirtualPersistence` (there is no `PersistMicroQc` action). |
| `ScheduleTimer` | Push to `VirtualTimer` queue; on fire → `TimerFired` enqueued with deterministic `deliver_at`. |
| `PersistMacroQc` | `store_macro_qc` on sender persistence (keeps port warm; macro logic 03c). |
| `UpdateBlobStatus` | Optional map on `World`. |
| Other broadcasts | 03b-1: debug assert or trace only. |

**`VirtualNet::enqueue_from_action`:** map `BroadcastMicroQc(qc)` → deliver `Event::MicroQcAssembled(qc)` to **all other** validators with `deliver_at = now` (zero additional delay in 03b-1).

### 5.6 Checkers (03b-1)

All predicates are on **L2 MicroQC** (not macro finality).

| Checker | Semantics |
|---------|-----------|
| `safety` | Across all validators' `VirtualPersistence`, no two distinct `MicroQc` values share the same `checkpoint_hash`. |
| `liveness` | After `R` sim rounds, ∃ validator where `micro_qc_for` returns `Some` for at least one hash (network-wide progress). |
| `lock_macro` | Remains **no-op** until 03c. Report must set `notes` to include `"lock_macro_skipped_until_03c"` (do not imply macro safety was checked). |

### 5.7 Tests (03b-1)

- **Unit (`consensus`):** `CertifiedVertexReceived` → non-empty actions with `BroadcastMicroQc`; `MicroQcAssembled` twice → second call empty.
- **Integration (`consensus/tests`):** 4-validator wave with relaxed commit; `HostContext` + in-memory `DagView`.
- **Sim:** `happy_path` → `liveness_ok && safety_ok` with note for lock_macro skip.
- **Sim driver:** after broadcast, sender persistence has MicroQC (`store_micro_qc` path).
- **Determinism:** same seed → same ordered list of action kinds per validator (golden or snapshot).
- **CLI:** `cargo build -p cli`; `replay_log` runs with stub context (may emit zero actions until replay format grows).

### 5.8 Acceptance criteria (03b-1)

- `cargo test -p consensus --locked` and `cargo test -p sim --locked` pass.
- `happy_path` report: `liveness_ok: true`, `safety_ok: true`, notes mention lock_macro skip.
- `StateMachine::step` non-empty on honest `CertifiedVertexReceived`.
- `Persistence::micro_qc_for` implemented and used by checkers.
- `cargo build --workspace --locked` passes (includes `node`, `cli`).
- No new dependencies inside `consensus` beyond existing workspace crates.

---

## 6. Phase 03b-2 — L2 Bullshark full

### 6.1 Purpose

Replace 03b-1 relaxed rules with whitepaper §8 modules under `crates/consensus/src/bullshark/`.

### 6.2 Scope

| Module | Deliverable |
|--------|-------------|
| `bullshark/wave.rs` | Unchanged API; used by commit |
| `bullshark/anchor.rs` | ECVRF sortition via `leader::vrf_sortition` + `RandomnessBeacon` |
| `bullshark/commit.rs` | Shortcut + slow path per `Config::bullshark` timing |
| `bullshark/linearize.rs` | BFS over `Closure(Aw)` via `DagView` |
| `bullshark/micro_qc.rs` | Real `try_finalize` stake threshold |
| `leader/` | Wire timing + sortition |
| `state_machine.rs` | Dispatch to modules; **delete** 03b-1 shortcut commit/linearize code paths (no dual-path flag) |

**Refactor rule:** 03b-2 **replaces** 03b-1 algorithm code in place. Keep 03b-1 tests that still apply; rewrite tests that encoded relaxed commit. Golden vectors regenerated with documented seeds.

**`Actions` capacity:** if any event can emit >8 actions, increase `SmallVec` cap or spill to `Vec` in 03b-2 (add regression test).

### 6.3 `sim` extensions (03b-2)

- Factory may emit multiple vertices per round where protocol requires `2f+1` participation.
- `VirtualNet`: optional latency/jitter from config (deterministic seeded).
- Scenarios: **`anchor_dos`**, **`network_partition`** — first real assertions (may `#[ignore]` in CI with issue link).
- `happy_path` still required green under full rules.

### 6.4 Acceptance criteria (03b-2)

- `bullshark/*` primary functions used by `step` are not skeleton `Ok(None)` on honest paths.
- ECVRF anchor in commit path (known seed test).
- ≥1 test for shortcut and ≥1 for slow path (shortened timing in test `Config`).
- `cargo test --workspace --locked` green; sim `happy_path` green; ≥1 additional L2 scenario green or explicitly ignored with issue link.

---

## 7. Data flow (end state 03b-1)

```text
┌─────────────┐     insert      ┌──────────────┐
│ Sim factory │ ───────────────►│ VirtualDag   │
└──────┬──────┘                 └──────▲───────┘
       │ sync CertifiedVertexReceived  │ DagView
       ▼ to all validators              │
┌──────────────┐   HostContext   ┌────┴─────────┐
│ VirtualNet   │◄── Actions ────│ StateMachine │
│ (delivery)   │── MicroQc ────►│   + Book     │
└──────────────┘   Assembled    └──────────────┘
       │
       ▼
  step: merge only (no re-broadcast if already sent)
```

Host persists sender MicroQC on `BroadcastMicroQc` before/while enqueueing to net.

---

## 8. `node` during Milestone A

- **03b-1 / 03b-2:** `node` compiles with `step(event, ctx)`.
- **Stub `HostContext` is allowed only when** `args.allow_skeleton_network` **or** `network_mode != "live"`. In **`live`** mode without skeleton flag, startup must use real `DagView` stub is **not** sufficient — prefer failing closed until **06b** wires ingress, or a minimal in-memory dag fed by future L1 adapter.
- Orchestrator does not need to show consensus progress until 06b; metrics may still show events processed.
- **Do not** block Milestone A on Docker e2e or gossip-driven QC visibility.
- **06b** (after 03b-1): timer dispatcher, `PersistMacroQc` / micro store from orchestrator if needed, validator set boot.

---

## 9. Error handling

- `step` returns `Err` only for invariant violations (unknown parent in dag, stake math, duplicate internal transition). Wire decode errors stay in `net`.
- Sim: log and continue on single-validator `step` error in 03b-1; scenario fails checkers if quorum/liveness not met.
- No panic in `step` on events from the honest factory (§5.4).

---

## 10. Testing strategy summary

| Level | 03b-1 | 03b-2 |
|-------|-------|-------|
| Unit | book, relaxed commit, micro qc emit, assembled idempotent | anchor, linearize BFS, commit slow |
| `consensus` integration | 4-validator wave, `micro_qc_for` | wave boundary, `2f+1` per round |
| `sim` scenario | `happy_path` + persistence after broadcast | + partition / anchor_dos |
| `cli` | stub ctx compile + replay | optional port snapshot replay |
| Property | optional | proptest where cheap |
| Golden | action-kind trace per seed | regen on algorithm replace |

---

## 11. Risks and mitigations

| Risk | Mitigation |
|------|------------|
| `step` API break | Ordered migration §3 including `cli` |
| 03b-1 vs 03b-2 semantic gap | Relaxed commit explicit; 03b-2 deletes shortcut code |
| Sim flake | Zero-delay net 03b-1; fixed tick order §5.5 |
| Liveness false negative | `store_micro_qc` on broadcast + `micro_qc_for` |
| `MicroQcAssembled` loop | Idempotent `Book` §5.2 |
| Scope creep L3 | Macro events no-op until 03c |
| `Actions` > 8 | Address in 03b-2 §6.2 |

---

## 12. Self-review

| Check | Result |
|-------|--------|
| Placeholders | Commit rule, tick order, persistence path, assembled semantics specified |
| Consistency | Checkers = MicroQC; factory matches relaxed commit |
| Scope | Milestone A bounded |
| Migration | Includes `cli`, port extension documented |
| Known deferrals | lock_macro, L1 verify, full 2f+1 per round → 03b-2 |

---

## 13. Next step

1. ~~Spec review~~ (revised 2026-05-19).
2. Implementation plans:
   - `docs/superpowers/plans/2026-05-19-03b1-l2-minimal-sim.md`
   - `docs/superpowers/plans/2026-05-19-03b2-l2-bullshark-full.md`
3. Execute 03b-1 first (`subagent-driven-development` or `executing-plans`).
