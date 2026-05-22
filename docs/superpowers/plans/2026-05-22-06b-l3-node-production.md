# L3 Node Production Wire (06b-l3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire `apps/node` so L3 macro-finality artifacts (proposals, partials, subnet aggregates, macro QCs, slash evidence) flow over live gossipsub, persist to RocksDB, fire macro timers, and are readable via JSON-RPC — using the real crypto and slash paths landed in **03d**.

**Architecture:** Split orchestrator side-effects into a dedicated **local action applier** (persistence, timers, blob status, beacon chain) while broadcasts continue through the existing `net_actions_tx` → `gossip_wire` path (already implemented in `crates/net`). Replace stub `HostContext` pieces: load validator set TOML with real `bls_pubkey` / `vrf_pubkey`, match `self_id` to an entry, and drive `DevSigner` from config/env. RPC reads finalized macro state through a thin `ConsensusQuery` adapter over `RocksPersistence`.

**Tech Stack:** Rust 1.85, `tokio`, `libp2p` gossipsub (`crates/net`), `consensus`, `storage` (RocksDB), `apps/node` (orchestrator, rpc, timer).

**Spec:** [`docs/superpowers/specs/2026-05-22-l3-macro-finality-design.md`](../specs/2026-05-22-l3-macro-finality-design.md) §4 row **06b-l3**; crypto/slash prerequisites in [`2026-05-22-l3-crypto-slashing-design.md`](../specs/2026-05-22-l3-crypto-slashing-design.md).

**Prerequisite:** **03d** complete (`cargo test -p consensus -p crypto -p sim -p types --locked`; sim scenarios green; `DevSigner` stub exists).

---

## Current gap (why this plan exists)

| Area | Today (post-03d) | 06b-l3 target |
|------|------------------|---------------|
| Gossip encode/decode L3 | ✅ `crates/net/src/gossip_wire.rs` | Keep; extend tests |
| Orchestrator broadcasts | ✅ `net_actions_tx` for `is_broadcast` actions | Keep |
| Local actions | ❌ `Bridge::translate_action` logs + drops persist/timer | ✅ `ActionApplier` → Rocks + timer dispatcher |
| Validator set in node | ❌ empty `ValidatorSet` in `runtime.rs` | ✅ TOML loader + pubkey match |
| Beacon | ❌ `FixedBeacon` constant | ✅ chain on `PersistMacroQc` (mirror sim) |
| Macro timers | ❌ `TokioClock` constructed but not wired to SM | ✅ schedule/cancel loop |
| RPC | ❌ skeleton returns `null` | ✅ `lua_getLatestFinalized`, `lua_getMacroCheckpointAt` |
| Swarm subscribe Mode A | ⚠️ only `BlsPartial(SubnetId(0))` hard-coded | ✅ subscribe `0..k_e` from config |
| `network_mode=live` | ❌ fails closed until 06b | ✅ passes when valset + applier + gossip green |

---

## Design decisions (lock-in)

| Topic | Decision |
|-------|----------|
| Local vs broadcast | Broadcasts → existing swarm channel; everything else → `ActionApplier` (no change to `consensus::Action` enum in 06b-l3) |
| Persistence | Reuse `RocksPersistence::{store_macro_checkpoint, store_macro_qc, append_slash_evidence}`; blob status column TBD — store in existing blob status map if present, else metrics-only stub with TODO for L4 |
| Validator identity | `self_id` = Blake3(`DEVNET_PEER_IDENTITY`, `node.identity.label`) **unchanged**; must appear in loaded valset TOML |
| Signer | Phase 1: extend `DevSigner` (file path + `LUA_DAG_BLS_KEY`); HSM out of scope |
| Beacon | `ChainedBeacon`: on each locally persisted macro QC, set `current = chain_beacon(prev, qc.checkpoint_hash)` |
| RPC | JSON-RPC 2.0 over existing axum server; methods read-only |
| Live mode gate | Remove `--allow-skeleton-network` requirement for L3 **after** Task 9 integration test passes; L1 `CertifiedVertex` ingress remains separate (future 06b-L1) |

---

## File map

| File | Action |
|------|--------|
| `apps/node/src/action_applier.rs` | **CREATE** — persist, timers, beacon, blob status |
| `apps/node/src/orchestrator.rs` | call `ActionApplier` for non-broadcast actions |
| `apps/node/src/runtime.rs` | load valset TOML; wire timer fan-in; pass applier handles |
| `apps/node/src/host_context.rs` | `ChainedBeacon`; real `CachedValidatorSet` |
| `apps/node/src/config.rs` | add `validator_set_path: PathBuf` |
| `apps/node/src/signer.rs` | map config label → key file; validate pubkey match |
| `apps/node/src/rpc_server.rs` | register L3 query methods |
| `apps/node/src/query.rs` | **CREATE** `RocksConsensusQuery` implementing `ConsensusQuery` |
| `crates/net/src/swarm_runner.rs` | subscribe all subnet partial topics from `k_e` |
| `crates/net/tests/gossip_l3_roundtrip.rs` | **CREATE** MacroProposal/MacroQc/SlashEvidence roundtrip |
| `apps/node/tests/l3_local_actions.rs` | **CREATE** persist + timer smoke |
| `docs/superpowers/specs/2026-05-22-l3-macro-finality-design.md` | link this plan; status bump |

---

### Task 1: `ActionApplier` skeleton

**Files:**
- Create: `apps/node/src/action_applier.rs`
- Modify: `apps/node/src/lib.rs`

- [ ] **Step 1: Failing test** in `apps/node/tests/l3_local_actions.rs`:

```rust
#[test]
fn applier_persists_macro_qc_and_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(storage::Database::open(dir.path()).unwrap());
    let p = storage::RocksPersistence::new(db);
    let mut applier = node::action_applier::ActionApplier::new(/* ... */);
    let cp = /* minimal MacroCheckpoint */;
    let qc = /* minimal MacroQc matching cp.hash */;
    applier.apply(&Action::PersistMacroCheckpoint(cp.clone())).unwrap();
    applier.apply(&Action::PersistMacroQc(qc.clone())).unwrap();
    assert_eq!(p.macro_checkpoint_at(cp.height).unwrap().unwrap().hash, cp.hash);
    assert_eq!(p.macro_qc_for(&cp.hash).unwrap().unwrap(), qc);
}
```

- [ ] **Step 2: Implement `ActionApplier`**

```rust
pub struct ActionApplier {
    persistence: RocksPersistence,
    timer_tx: mpsc::Sender<(TimerId, u128)>,
    beacon: Arc<Mutex<ChainedBeacon>>,
}

impl ActionApplier {
    pub fn apply(&mut self, action: &Action) -> anyhow::Result<()> {
        match action {
            Action::PersistMacroCheckpoint(cp) => {
                self.persistence.store_macro_checkpoint(cp)?;
            }
            Action::PersistMacroQc(qc) => {
                self.persistence.store_macro_qc(qc)?;
                if let Ok(mut b) = self.beacon.lock() {
                    b.adopt_macro_qc(qc);
                }
            }
            Action::EmitSlashEvidence { evidence, .. } => {
                self.persistence.append_slash_evidence(evidence)?;
            }
            Action::ScheduleTimer { id, delay_nanos } => {
                let _ = self.timer_tx.try_send((*id, *delay_nanos));
            }
            Action::CancelTimer(id) => { /* track in applier or timer task */ }
            Action::UpdateBlobStatus { blob, status } => { /* store or metrics */ }
            _ => {}
        }
        Ok(())
    }
}
```

- [ ] **Step 3: Run test** — expect PASS

```bash
cargo test -p node l3_local_actions --locked
```

- [ ] **Step 4: Commit**

```bash
git add apps/node/src/action_applier.rs apps/node/tests/l3_local_actions.rs
git commit -m "feat(node): ActionApplier for L3 local actions (06b-l3)"
```

---

### Task 2: Orchestrator wiring

**Files:**
- Modify: `apps/node/src/orchestrator.rs`

- [ ] **Step 1: Replace `Bridge::translate_action` call** with `action_applier.apply(&action)` for non-broadcast branch.

- [ ] **Step 2: Keep broadcast branch** unchanged (`net_actions_tx.try_send`).

- [ ] **Step 3: Run**

```bash
cargo test -p node --locked
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(node): route local SM actions through ActionApplier (06b-l3)"
```

---

### Task 3: Timer dispatcher loop

**Files:**
- Modify: `apps/node/src/runtime.rs`, `apps/node/src/timer.rs`

- [ ] **Step 1: Spawn timer task**

```rust
let (timer_schedule_tx, mut timer_schedule_rx) = mpsc::channel(256);
let events_tx_timer = events_tx.clone();
tokio::spawn(async move {
    while let Some((id, delay)) = timer_schedule_rx.recv().await {
        crate::timer::schedule(events_tx_timer.clone(), id, delay);
    }
});
```

Adapt `schedule` to emit `Event::TimerFired(id)` on `events_tx` instead of raw `TimerId` (extend signature once).

- [ ] **Step 2: Unit test** — `timer.rs` already has `schedule_fires_in_order`; add integration asserting `Event::TimerFired` reaches a mock channel.

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(node): wire macro/bullshark timers to SM events (06b-l3)"
```

---

### Task 4: Validator set bootstrap

**Files:**
- Modify: `apps/node/src/config.rs`, `apps/node/src/runtime.rs`, `config/profiles/*.toml`

- [ ] **Step 1: Add config field**

```toml
# config/profiles/devnet.toml
validator_set_path = "config/valsets/devnet-4.toml"
```

- [ ] **Step 2: Load in `run_async`**

```rust
let valset = validator_set_loader::load_from_toml(&cfg.validator_set_path)?;
let self_id = validator_id_from_label(&cfg.node.identity.label);
let _ = valset.entries.iter().find(|e| e.id == self_id)
    .ok_or_else(|| anyhow!("self_id not in validator set"))?;
let host_bundle = StubHostBundle::new(valset);
```

- [ ] **Step 3: Ship sample valset TOML** with `bls_pubkey` / `vrf_pubkey` bytes matching sim devnet keys (document derivation in comment).

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(node): load validator set TOML for L3 verify (06b-l3)"
```

---

### Task 5: Signer pubkey alignment

**Files:**
- Modify: `apps/node/src/signer.rs`, `apps/node/src/host_context.rs`

- [ ] **Step 1: After loading secret key, assert** `sk.public().to_bytes() == entry.bls_pubkey` for `self_id`.

- [ ] **Step 2: Support `LUA_DAG_BLS_KEY` path** (hex or raw 32-byte file) documented in `apps/node/README` snippet inside plan commit message body only (no new markdown file unless requested).

- [ ] **Step 3: Test** — unit test in `signer.rs` with temp key file.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(node): DevSigner validates against valset pubkey (06b-l3)"
```

---

### Task 6: `ChainedBeacon`

**Files:**
- Modify: `apps/node/src/host_context.rs`

- [ ] **Step 1: Implement**

```rust
pub struct ChainedBeacon {
    current: Hash32,
}
impl ChainedBeacon {
    pub fn adopt_macro_qc(&mut self, qc: &MacroQc) {
        self.current = consensus::leader::beacon::chain_beacon(&self.current, &qc.checkpoint_hash);
    }
}
impl RandomnessBeacon for ChainedBeacon { /* current() returns self.current */ }
```

- [ ] **Step 2: Wire through `ActionApplier` on `PersistMacroQc`** (Task 1).

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(node): chain macro beacon on QC persist (06b-l3)"
```

---

### Task 7: Swarm — subscribe all BLS partial subnets

**Files:**
- Modify: `crates/net/src/swarm_runner.rs`

- [ ] **Step 1: Replace hard-coded `BlsPartial(SubnetId(0))`** with loop `0..k_e` from net config (add `macro_subnet_count: u32` to `NetConfig` defaulting from consensus `compute_ke` at node startup).

- [ ] **Step 2: Test** in `crates/net/tests/gossip_l3_roundtrip.rs`:

```rust
#[test]
fn macro_proposal_roundtrips_on_wire() {
    let proposal: MacroProposal = /* fixture with real-ish sig bytes */;
    let (topic, bytes) = gossip_wire::outbound_broadcast(&Action::BroadcastMacroProposal(proposal.clone())).unwrap().unwrap();
    let ev = gossip_wire::inbound_event(&topic.wire_name(), &bytes).unwrap().unwrap();
    assert!(matches!(ev, Event::MacroProposalReceived(p) if p == proposal));
}
```

Repeat for `MacroQc`, `SlashEvidence`, `SubnetAggregate`.

- [ ] **Step 3: Run**

```bash
cargo test -p net gossip_l3 --locked
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(net): L3 gossip roundtrip tests + subnet subscriptions (06b-l3)"
```

---

### Task 8: JSON-RPC L3 queries

**Files:**
- Create: `apps/node/src/query.rs`
- Modify: `apps/node/src/rpc_server.rs`

- [ ] **Step 1: Implement `RocksConsensusQuery`**

```rust
impl ConsensusQuery for RocksConsensusQuery {
    fn latest_finalized(&self) -> Result<Option<MacroQc>> {
        /* scan macro QC column or maintain head pointer in applier */
    }
}
```

Pragmatic v1: scan heights `0..128` via `macro_checkpoint_at` + `macro_qc_for`, pick highest height with blob status `Finalized` if stored; else highest `Justified`.

- [ ] **Step 2: RPC methods**

| Method | Returns |
|--------|---------|
| `lua_getLatestFinalized` | `{ checkpoint_hash, height, mode }` or `null` |
| `lua_getMacroCheckpointAt` | Borsh-base64 checkpoint at height param |

- [ ] **Step 3: Test** — axum oneshot request in `apps/node/tests/l3_local_actions.rs`.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(node): L3 JSON-RPC query methods (06b-l3)"
```

---

### Task 9: Node integration smoke

**Files:**
- Create: `apps/node/tests/l3_gossip_smoke.rs`

- [ ] **Step 1: Two-node loopback test** (pattern from `crates/net/tests/devnet_loopback_gossip.rs`):

1. Start two swarms with known labels from valset TOML.
2. Node A SM emits `BroadcastMacroProposal` via manual step or injected event.
3. Node B receives `MacroProposalReceived` on events channel within timeout.

- [ ] **Step 2: Run**

```bash
cargo test -p node l3_gossip_smoke --locked
```

- [ ] **Step 3: Relax live guard** in `runtime.rs` comment + condition:

```rust
// L3 host wiring complete in 06b-l3; L1 ingress still skeleton.
if cfg.node.network_mode == "live" && !args.allow_skeleton_network && !cfg.node.l3_wire_complete {
    anyhow::bail!(/* ... */);
}
```

Set `l3_wire_complete = true` in devnet profile after smoke passes.

- [ ] **Step 4: Commit**

```bash
git commit -m "test(node): L3 gossip smoke + live gate flag (06b-l3)"
```

---

### Task 10: Acceptance + docs

- [ ] **Step 1: Workspace verify**

```bash
cargo fmt --check
cargo clippy -p node -p net -p consensus --all-targets --locked -- -D warnings
cargo test -p consensus -p crypto -p net -p node -p sim --locked
```

- [ ] **Step 2: Manual devnet checklist**

```bash
cargo run -p node --release --locked -- --profile devnet --allow-skeleton-network
# in another shell:
curl -s -X POST http://127.0.0.1:8545/ -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"lua_getLatestFinalized","params":[]}'
```

- [ ] **Step 3: Update parent spec** — in `2026-05-22-l3-macro-finality-design.md` §4, set **06b-l3** path to this file; status note `06b-l3 plan ready`.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/
git commit -m "docs: link 06b-l3 node production plan"
```

---

## Done — 06b-l3 acceptance criteria

- Node persists `MacroCheckpoint`, `MacroQc`, and `SlashEvidence` locally when SM emits actions.
- Macro / Mode B timers schedule and fire `Event::TimerFired` into the SM loop.
- Validator set TOML supplies pubkeys used by L3 verify paths; local signer matches `self_id` entry.
- Beacon chains on macro QC adoption (matches sim behavior).
- Gossip roundtrip tests cover all L3 broadcast action types.
- JSON-RPC returns non-null finalized head after a devnet run (manual or automated smoke).
- Sim L3 scenarios remain green (no consensus algorithm changes).

**Next:** **06b-L1** certified-vertex ingress, or **03d+** inactivity leak emission ([plan](./2026-05-22-03d-plus-inactivity-leak.md)).
