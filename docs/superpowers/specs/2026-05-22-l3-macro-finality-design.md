# Design: Milestone B — L3 Macro-Finality on `sim` (phased 03c-1 / 03c-2)

**Date:** 2026-05-22
**Status:** 03c-2 landed; 03d landed; 06b-l3 landed; **03d+ landed**; 06b-L1 landed (see plan table below)
**Audience:** Contributors implementing macro-finality + sim driver
**Relations:** Extends [`2026-05-11-folder-architecture-design.md`](2026-05-11-folder-architecture-design.md) and [`2026-05-19-l2-sim-milestone-a-design.md`](2026-05-19-l2-sim-milestone-a-design.md). Does **not** include `T_macropropose` backup proposer, Mode A subnet rotation, Mode B leaderless fallback, real BLS sign/verify, surround/double-vote detection, inactivity leak, or L4 BTC anchor — those are 03c-2 and 03d.

---

## 1. Goals

Land **L3 macro-finality** on the existing pure SM + `sim`, mirroring how Milestone A landed L2:

- `consensus` stays pure (no tokio, no libp2p, no rocksdb); side effects remain `Action` values.
- `apps/sim` `happy_path` drives every honest validator from `accepted → soft_confirmed → justified → finalized` across multiple macro windows.
- `apps/sim/checker/lock_macro.rs` replaces its current stub with a real cross-validator agreement check.
- Deliverable in **two phases**:
  - **03c-1 (minimal vertical slice):** macro window cadence (`W=8`), round-robin proposer, Mode 0 flat aggregation, 2-chain rule, `lock_macro` driven from SM, fixture BLS.
  - **03c-2 (Bullshark-equivalent full):** real ECVRF proposer sortition + Shoal reputation, backup proposer + `T_macropropose` timeout, Mode A subnet rotation, Mode B leaderless fallback, additional sim scenarios.

**Non-goals (03c):**

- L4 BTC anchor, light-client sync, DKG ceremony.
- Real surround/double-vote evidence emission, inactivity leak, real BLS sign/verify — covered by **03d** (separate spec).
- New L1 work; `CertifiedVertex` continues to come from `sim::VirtualDag`.
- Production wire-up in `apps/node` for L3 RPC visibility — follow-up after 03c-1 lands, gated like the L2 work was in **06b**.

---

## 2. Context (current repo, 2026-05-22)

| Layer | State |
|-------|-------|
| `crates/consensus` | L2 (Bullshark) full path landed; `state_machine::step` dispatches `bullshark::on_certified_vertex / on_micro_qc_assembled / on_timer_fired`. Macro events (`MacroProposalReceived`, `BlsPartialReceived`, `SubnetAggregateReceived`, `MacroQcReceived`) return empty `Actions`. `Event::SlashEvidenceFound` returns empty (→ 03d). |
| `crates/consensus/src/macro_fin/` | Stubs only: `window` (round → height), `proposer` (struct), `checkpoint::CheckpointBuilder::try_build → Ok(None)`, `macro_qc::MacroQcAssembler::try_finalize → Ok(None)`, `two_chain` (justified head field only), `vote_book` (record/get only, no detection), aggregation `{mode0_flat,mode_a_subnet,mode_b_leaderless,subnet}` (zero-sized structs except `subnet` which has a working `index_for`). |
| `crates/consensus/src/lock_macro.rs` | `LockMacro::try_lock` works, including conflict rejection. Not called from any SM dispatch arm. |
| Types | `MacroCheckpoint`, `MacroQc` (with `AggregationMode` discriminant), `MacroProposal` (with `VrfProof` + `BlsSig`), `MacroHeader` (forward-compat), `SlashEvidence` variants all defined. |
| `Persistence` port | `store_macro_checkpoint`, `store_macro_qc`, `macro_checkpoint_at`, `macro_qc_for` already present. |
| `apps/sim` | `World::apply_actions` handles L2 actions; `BroadcastMacroProposal / BroadcastBlsPartial / BroadcastSubnetAggregate / BroadcastMacroQc / EmitSlashEvidence` trigger `debug_assert!(false, "unexpected non-L2 action in 03b-1")`. `checker::lock_macro::check` returns `Ok(())` unconditionally. |
| `apps/node`, `apps/cli` | Compile against current `StateMachine::new(cfg)` signature; both construct SMs at startup. |

One implicit gap that shapes this design: bullshark currently emits `MicroQc` per wave commit but never materializes a `MicroCheckpoint` artifact. The L3 `MacroCheckpoint.micro_root` is therefore derived from a rolling buffer of `MicroQc.checkpoint_hash` values (§8); no new `Action::PersistMicroCheckpoint` is introduced.

---

## 3. Architectural decision: direct integration in `StateMachine::step`

L3 hooks into the existing dispatch in `state_machine.rs` as additional match arms plus a chained call after bullshark commits. `MacroBook` mirrors `WaveBook` as in-memory SM state.

```text
StateMachine {
    cfg: Config,
    emitted: EmittedSet,       // bullshark (existing)
    waves:   WaveBook,         // bullshark (existing)
    macros:  MacroBook,        // NEW — L3 (§6)
}
```

`step` chains macro-finality work after bullshark on `CertifiedVertexReceived`, and dispatches the four macro events (`MacroProposalReceived`, `BlsPartialReceived`, `MacroQcReceived`, `SubnetAggregateReceived → empty in 03c-1`) into `macro_fin::on_*` entry points. **No new ports, no `step` signature change.** One signature change does propagate outward: `StateMachine::new(cfg)` becomes `StateMachine::new(cfg, self_id)` because `MacroBook` needs to know which validator it speaks for (proposer self-check + `lock_macro` caller). Three call sites migrate: `apps/sim/src/world.rs`, `apps/node/src/runtime.rs`, `apps/cli/src/commands/replay_log.rs`.

**One new `Action` variant:** `Action::PersistMacroCheckpoint(MacroCheckpoint)`. The `Persistence::store_macro_checkpoint` port already exists; the new variant just lets the SM trigger it next to `PersistMacroQc`. This gives the sim `lock_macro` checker per-height visibility (§5.10), and gives downstream consumers (RPC, light-client header builder) a canonical `MacroCheckpoint` artifact to read back. Adding a variant is wire-breaking, but L3 is not yet on the wire.

**Rejected alternatives:**

- **Bullshark returns structured `WaveOutcome`.** Explicit cross-module handoff at the cost of changing three bullshark entry-point signatures and threading an `Option` through every dispatch arm. Marginal clarity over A.
- **Macro layer pulls via `Persistence::micro_qc_for`.** Avoids cross-module wiring but adds per-event persistence scans and a host-ordering invariant (host must apply `store_micro_qc` between SM steps for `step` to see its own output). Couples L3 correctness to host behavior.

---

## 4. Plan file decomposition

| ID | Path | Depends on |
|----|------|------------|
| **03c-1** | `docs/superpowers/plans/2026-05-22-03c1-l3-minimal-sim.md` | L2 Milestone A complete |
| **03c-2** | `docs/superpowers/plans/2026-05-22-03c2-l3-full.md` | 03c-1 complete |
| **03d** | `docs/superpowers/plans/2026-05-22-03d-l3-crypto-slashing.md` | 03c-2 complete |
| **03d+** | `docs/superpowers/plans/2026-05-22-03d-plus-inactivity-leak.md` | 03d complete |
| **06b-l3** | `docs/superpowers/plans/2026-05-22-06b-l3-node-production.md` | 03d complete |

Follow-on (not Milestone B core):

| ID | Topic |
|----|-------|
| **06b-L1** | Certified-vertex ingress + live DAG view (separate from 06b-l3) |
| **L4** | BTC anchor, light-client sync |

---

## 5. Phase 03c-1 — L3 minimal vertical slice

### 5.1 Purpose

Prove the **W-wave commit → MacroProposal → BlsPartial → MacroQc → 2-chain → Finalized** loop with deterministic state mutations and real `BroadcastMacroProposal` / `BroadcastBlsPartial` / `BroadcastMacroQc` traffic, without ECVRF, backup proposer, subnet rotation, or real BLS.

### 5.2 Algorithm simplifications (explicit)

Let `n` = validator count, `f = (n - 1) / 3`, `W = cfg.macro_fin.macro_window_w = 8`.

| Whitepaper feature | 03c-1 behavior |
|--------------------|----------------|
| Proposer selection | **Round-robin:** `primary = validators[h mod n]` for macro height `h`. `backup` is recorded in `ProposerSchedule` but unused (no `T_macropropose` timer in 03c-1). |
| Aggregation mode | **Mode 0 flat only.** SM hard-codes `AggregationMode::Mode0Flat`; `select_mode` is left as-is and not called from 03c-1 hot paths. |
| BlsPartial signing | **Fixture sig:** `BlsSig` from `blake3_with_dst(VALIDATOR_BLS_PARTIAL, validator_id ‖ checkpoint_hash)` truncated/zero-padded to 96 bytes. No `crypto::bls::sign`. |
| MacroQc aggregation | Every validator independently aggregates. When ≥ `2f + 1` distinct validators' `BlsPartial`s for the **same** `checkpoint_hash` are observed, emit `BroadcastMacroQc(qc)` **once per height per validator** (guarded by `MacroBook.emitted_macro_qc`). `agg.sig = BlsSig([0xCD; 96])` fixture (distinct from MicroQc's `0xAB` for trace clarity); `bitmap` set from contributor indices. |
| 2-chain rule | `justified(h)` when `MacroQc(h)` is adopted; `finalized(h-1)` when `MacroQc(h)` is adopted **and** `MacroCheckpoint(h).parent == MacroCheckpoint(h-1).hash` (no skip). |
| `MacroQcReceived` | **Idempotent merge:** if `qc.checkpoint_hash` already locally adopted, return empty `Actions`. Otherwise persist + advance 2-chain (no re-broadcast). |
| ECVRF / Shoal reputation | Not used. `vrf_proof = VrfProof::zero()`. |
| Mode A subnet rotation | Not used. `Event::SubnetAggregateReceived → Ok(Actions::new())`. |
| Mode B leaderless | Not used. |
| Slashing | `lock_macro.try_lock(self_id, height, checkpoint)` called **before** emitting a `BlsPartial`. On `Err`, the partial is suppressed and `MacroBook.suppressed_conflicts` is incremented (stats only, not gossiped). Surround / double-vote / `EmitSlashEvidence` → 03d. |
| Inactivity leak | Not exercised. |
| Validator-set epoch | Static `Epoch(0)` (matches L2 minimal). |
| `BlobStatus::EpochFinalized` | Deferred to L4. 03c-1 reaches `Finalized` only. |

### 5.3 `MacroBook` state (03c-1)

In a new file `crates/consensus/src/macro_fin/book.rs`:

```rust
pub struct MacroBook {
    /// Validator this book belongs to; needed for proposer self-check + lock_macro caller.
    self_id: ValidatorId,
    /// Rolling buffer of the last W locally-emitted MicroQc.checkpoint_hash values.
    /// Drained when len == W to build the next MacroCheckpoint candidate.
    micro_ring: VecDeque<Hash32>,
    /// Monotonic next macro height to propose / vote on.
    /// Bootstrap: Height(0) covers waves [0..W); genesis parent = Hash32::zero().
    next_height: Height,
    /// Hash of the most recently locally-adopted MacroCheckpoint (parent for next candidate).
    last_macro_hash: Hash32,
    /// Pending candidates indexed by height (gathered from MacroProposal or built locally).
    candidate: BTreeMap<Height, MacroCheckpoint>,
    /// Partial signers per checkpoint_hash (validator-id set; replaces bitmap until aggregate time).
    partials: HashMap<Hash32, BTreeSet<ValidatorId>>,
    /// Set of checkpoint_hashes this validator already emitted a BroadcastMacroQc for.
    emitted_macro_qc: HashSet<Hash32>,
    /// 2-chain head + finality state.
    two_chain: TwoChainRule,
    /// Per-validator lock tracker driven on every BlsPartial emission.
    locks: LockMacro,
    /// Non-protocol stat: number of times try_lock rejected a proposal.
    suppressed_conflicts: u64,
}
```

`MacroBook::new(self_id)` initializes `next_height = Height(0)`, `last_macro_hash = Hash32::zero()`. All mutations happen inside `macro_fin::on_*` entry points; no hidden globals.

### 5.4 `step` dispatch changes

```rust
pub fn step(&mut self, event: Event, ctx: &HostContext<'_>) -> Result<Actions> {
    match event {
        Event::CertifiedVertexReceived(cv) => {
            let mut actions = bullshark::on_certified_vertex(
                &mut self.emitted, &mut self.waves, &self.cfg, cv, ctx)?;
            // L3 chain-in: scans `actions` for BroadcastMicroQc variants and appends
            // any resulting macro actions in-place. Single &mut borrow; no aliasing.
            macro_fin::on_local_micro_qcs(
                &mut self.macros, &self.cfg, ctx, &mut actions)?;
            Ok(actions)
        }
        Event::MicroQcAssembled(qc) => bullshark::on_micro_qc_assembled(&self.emitted, qc),
        Event::TimerFired(id) => bullshark::on_timer_fired(
            &mut self.emitted, &mut self.waves, &self.cfg, id, ctx),

        Event::MacroProposalReceived(p) =>
            macro_fin::on_macro_proposal(&mut self.macros, &self.cfg, p, ctx),
        Event::BlsPartialReceived(bp) =>
            macro_fin::on_bls_partial(&mut self.macros, &self.cfg, bp, ctx),
        Event::SubnetAggregateReceived(_) => Ok(Actions::new()),   // Mode A → 03c-2
        Event::MacroQcReceived(qc) =>
            macro_fin::on_macro_qc_received(&mut self.macros, qc, ctx),

        Event::ValidatorSetUpdated { .. } => Ok(Actions::new()),
        Event::SlashEvidenceFound(_) => Ok(Actions::new()),        // 03d
    }
}
```

`macro_fin::on_local_micro_qcs` walks **only** the `Action::BroadcastMicroQc` variants already present in the `&mut Actions` slice (a borrow snapshot of `.iter().filter(...)` is collected before appending), and pushes any resulting macro actions onto the same slice. This is the single integration point between bullshark and macro_fin.

### 5.5 Data flow (end-state 03c-1)

```text
                  StateMachine::step
                          │
                          ▼ Event::CertifiedVertexReceived(cv)
            bullshark::on_certified_vertex
                          │
              Actions: [..., BroadcastMicroQc(qc)?]
                          │
                          ▼
         macro_fin::on_local_micro_qcs
         ├─ micro_ring.push(qc.checkpoint_hash)
         ├─ if ring.len() == W:
         │     micro_root  = H(MACRO_MICRO_ROOT ‖ concat(W))
         │     candidate   = build(h+1, micro_root, last_macro_hash, epoch)
         │     ring.clear()
         │     if self_id == round_robin_primary(h+1):
         │         emit BroadcastMacroProposal(MacroProposal{
         │             checkpoint, proposer = self,
         │             vrf_proof = ZERO_VRF,
         │             proposer_sig = fixture })

                          ▼ Event::MacroProposalReceived(p)
         macro_fin::on_macro_proposal
         ├─ proposer == round_robin_primary(p.height)?              else drop
         ├─ p.checkpoint.parent == last_macro_hash?                  else drop
         ├─ p.checkpoint.micro_root == local_ring_root_for(height)?  else drop
         ├─ lock_macro.try_lock(self_id, height, p.checkpoint.hash):
         │     Err  → suppressed_conflicts += 1, empty
         │     Ok   →
         │       candidate.insert(height, p.checkpoint)
         │       partials[p.checkpoint.hash].insert(self_id)
         │       emit BroadcastBlsPartial{ SubnetId(0), self, hash, fixture_sig }
         │       emit UpdateBlobStatus(SoftConfirmed)               // once per checkpoint

                          ▼ Event::BlsPartialReceived(bp)
         macro_fin::on_bls_partial
         ├─ ignore if bp.subnet != SubnetId(0)
         ├─ ignore if no candidate registered for bp.checkpoint_hash
         ├─ partials[hash].insert(bp.validator)
         ├─ if |partials| >= 2f+1 and !emitted_macro_qc(hash):
         │     qc = macro_qc::try_finalize_mode0(hash, partials, set).expect("threshold met")
         │     emitted_macro_qc.insert(hash)
         │     two_chain.adopt(candidate[height])
         │     last_macro_hash = candidate[height].hash
         │     next_height     = Height(height.0 + 1)
         │     emit BroadcastMacroQc(qc),
         │          PersistMacroCheckpoint(candidate[height]),
         │          PersistMacroQc(qc),
         │          UpdateBlobStatus(Justified)
         │     if two_chain.newly_finalized_height() == Some(prev_h):
         │         emit UpdateBlobStatus(Finalized for prev_h)

                          ▼ Event::MacroQcReceived(qc)
         macro_fin::on_macro_qc_received
         ├─ if emitted_macro_qc.contains(qc.checkpoint_hash) → empty
         ├─ emitted_macro_qc.insert(qc.checkpoint_hash)
         ├─ two_chain.adopt(candidate[height])      // candidate must exist; else log+drop
         ├─ last_macro_hash = candidate[height].hash
         └─ emit PersistMacroCheckpoint(candidate[height]),
              PersistMacroQc(qc), UpdateBlobStatus(Justified)
              + UpdateBlobStatus(Finalized) if 2-chain advances
```

`BlobId` for L3 `UpdateBlobStatus` is the deterministic projection `BlobId::from(MacroCheckpoint.hash.0[..16])`. Documented placeholder: real per-blob granularity arrives when L1 lands.

### 5.6 `macro_fin` module surface (per-file)

| File | 03c-1 responsibility |
|------|----------------------|
| `macro_fin/mod.rs` | Public entry points `on_local_micro_qcs`, `on_macro_proposal`, `on_bls_partial`, `on_macro_qc_received`. Re-exports `MacroBook`, `MacroWindow`, `ProposerSchedule`, `TwoChainRule`, `VoteBook`, `AggregationMode`, `select_mode`. |
| `macro_fin/book.rs` | **NEW.** `MacroBook` struct (§5.3) + helpers `micro_root_of_ring(&VecDeque<Hash32>) -> Hash32` and `build_candidate(height, epoch, parent, micro_root) -> MacroCheckpoint`. Pure data; no I/O. |
| `macro_fin/window.rs` | No change in 03c-1. |
| `macro_fin/proposer.rs` | Add `ProposerSchedule::round_robin(set: &ValidatorSet, height: Height) -> Self`. 03c-2 adds `ProposerSchedule::vrf_sortition(...)`. |
| `macro_fin/checkpoint.rs` | Replace `CheckpointBuilder::try_build` with free function `pub fn build(height, epoch, parent, micro_root) -> MacroCheckpoint` that fills the struct and computes `hash = blake3_with_dst(MACRO_CHECKPOINT, canonical_bytes(self_without_hash))`. |
| `macro_fin/macro_qc.rs` | Replace `MacroQcAssembler::try_finalize` skeleton with free function `pub fn try_finalize_mode0(target: Hash32, signers: &BTreeSet<ValidatorId>, set: &ValidatorSet) -> Option<MacroQc>`. Returns `Some` iff `signers.len() >= 2f+1`. |
| `macro_fin/two_chain.rs` | Expand to `TwoChainRule { adopted: BTreeMap<Height, MacroCheckpoint>, justified_head: Option<Hash32>, finalized_head: Option<Hash32> }`. Methods: `adopt(cp)`, `is_justified(h)`, `newly_finalized_height() -> Option<Height>`. |
| `macro_fin/vote_book.rs` | Add `record(validator, VoteRecord)` invoked from `on_macro_proposal` after a successful `try_lock` (same site that emits the partial). Detection logic remains stub (→ 03d). |
| `macro_fin/aggregation/mod.rs` | No change in 03c-1; `select_mode` thresholds-only. SM hard-codes `AggregationMode::Mode0Flat`. |
| `macro_fin/aggregation/{mode0_flat,mode_a_subnet,mode_b_leaderless,subnet}.rs` | Untouched in 03c-1 (`subnet::SubnetAssign` already works). Filled by 03c-2. |
| `lock_macro.rs` | No code change; gains a caller. |
| `crypto::hash::dst` | Add constants `MACRO_CHECKPOINT`, `MACRO_MICRO_ROOT`, `VALIDATOR_BLS_PARTIAL`, `MACRO_PROPOSER_SIG`. |

### 5.7 `micro_root` derivation

```rust
// in macro_fin/book.rs
pub fn micro_root_of_ring(cfg: &Config, ring: &VecDeque<Hash32>) -> Hash32 {
    debug_assert_eq!(ring.len() as u32, cfg.macro_fin.macro_window_w);
    let mut buf = Vec::with_capacity(32 * ring.len());
    for h in ring { buf.extend_from_slice(&h.0); }
    crypto::hash::blake3_with_dst(dst::MACRO_MICRO_ROOT, &buf)
}
```

Order = local emission order. Under 03c-1's honest factory (vertices delivered synchronously to every validator in identical order) every validator produces the same `micro_root` for the same height; the proposer's value and the verifier's local value match.

### 5.8 Round-robin proposer

```rust
// in macro_fin/proposer.rs
impl ProposerSchedule {
    pub fn round_robin(set: &ValidatorSet, height: Height) -> Self {
        debug_assert!(!set.entries.is_empty());
        let n = set.entries.len();
        let primary = set.entries[(height.0 as usize) % n].id;
        let backup  = set.entries[((height.0 as usize) + 1) % n].id;
        Self { height, primary, backup }
    }
}
```

Self-identity gating: after the W-th local MicroQc, every validator builds the same `MacroCheckpoint` candidate but only the validator with `self_id == schedule.primary` emits `BroadcastMacroProposal`. Non-proposers stash the candidate and wait for the proposal event to arrive over the wire.

### 5.9 2-chain rule (Casper FFG, simplified)

```rust
// in macro_fin/two_chain.rs
pub fn newly_finalized_height(&self) -> Option<Height> {
    let head = self.justified_head_height()?;
    let prev = Height(head.0.checked_sub(1)?);
    let head_cp = self.adopted.get(&head)?;
    let prev_cp = self.adopted.get(&prev)?;
    if head_cp.parent == prev_cp.hash && self.finalized_head != Some(prev_cp.hash) {
        Some(prev)
    } else { None }
}
```

Justification depth = 1 macro window; finality depth = 2. No source/target epoch arithmetic in 03c-1 — `VoteRecord { source, target, checkpoint }` recorded in `vote_book` uses `source = Epoch(0), target = Epoch(0)`. 03d uses real source/target for surround scans.

### 5.10 `lock_macro` wiring

**SM side:** the only mutation of `LockMacro` in 03c-1 is the call inside `on_macro_proposal` **before** emitting `BlsPartial`. On `Err`, increment `macros.suppressed_conflicts` and return empty actions for that event.

**Sim side:** `apps/sim/src/checker/lock_macro.rs` replaces the stub with an observational check that iterates `Persistence::macro_checkpoint_at(h)` for `h = 0..max_height_seen` across all validators and asserts: for every height, every validator that adopted a `MacroQc` for that height adopted the same `(checkpoint_hash, MacroQc)`. The per-height grouping requires `Action::PersistMacroCheckpoint` to be wired (see §5.11). `suppressed_conflicts` is not part of the protocol invariant — checker is purely about adopted artifacts.

### 5.11 `sim` driver changes (03c-1)

**`apps/sim/src/world.rs` — `apply_actions`:**

```rust
Action::BroadcastMacroProposal(p) =>
    self.net.enqueue_from_action(validator_idx, &Action::BroadcastMacroProposal(p),
        self.machines.len() as u32, now, &mut self.rng),
Action::BroadcastBlsPartial(bp) =>
    self.net.enqueue_from_action(validator_idx, &Action::BroadcastBlsPartial(bp),
        self.machines.len() as u32, now, &mut self.rng),
Action::BroadcastMacroQc(qc) => {
    self.persistence[validator_idx as usize].store_macro_qc(&qc)?;
    self.net.enqueue_from_action(validator_idx, &Action::BroadcastMacroQc(qc),
        self.machines.len() as u32, now, &mut self.rng);
}
Action::PersistMacroCheckpoint(cp) =>
    self.persistence[validator_idx as usize].store_macro_checkpoint(&cp)?,
Action::PersistMacroQc(qc) =>
    self.persistence[validator_idx as usize].store_macro_qc(&qc)?,
Action::UpdateBlobStatus { blob, status } =>
    self.persistence[validator_idx as usize].update_blob_status(blob, status),
Action::BroadcastSubnetAggregate(_) => debug_assert!(false, "ModeA → 03c-2"),
Action::EmitSlashEvidence { .. }    => debug_assert!(false, "slashing → 03d"),
```

The existing catch-all `debug_assert!(false, "unexpected non-L2 action in 03b-1")` is removed; only the two genuinely-out-of-scope variants keep the panic guard.

**`apps/sim/src/virtual_net.rs`:** `enqueue_from_action` learns three new variants. All deliver to "all other validators" with the same latency/jitter rules as `BroadcastMicroQc`. For `BroadcastBlsPartial` the network applies per-recipient jitter with deterministic-by-`(sender_idx, recipient_idx)` ordering — pinned by the determinism golden (§6).

**`apps/sim/src/virtual_persistence.rs`:** gains an in-memory `BTreeMap<BlobId, BlobStatus>` and a monotonic `update_blob_status(blob, status)` (no downgrade). Adds `pub fn blob_status(&self, blob: &BlobId) -> Option<BlobStatus>` and `pub fn finalized_count(&self) -> usize` for checkers.

**`apps/sim/src/world.rs`:** `StateMachine::new(cfg.clone(), entries[i].id)` per §3 breaking signature.

**`apps/sim/src/scenarios/happy_path.rs`:** bump default rounds to ≥ 80; replace the note `"lock_macro_skipped_until_03c"` with `"l3_finality_active"`.

### 5.12 Checkers (03c-1)

| Checker | Semantics |
|---------|-----------|
| `safety` | (extend) For every macro height with adopted MacroQcs in any persistence, all adopted `MacroQc.checkpoint_hash` values for that height are identical across validators. (L2 MicroQc safety check remains.) |
| `liveness` | (extend) After `R` rounds, ≥1 validator has `finalized_count() >= 1`. With default config (`W=8`, 4-round waves) one full finality cycle is 2 macro windows = 64 micro-rounds; default scenario rounds bumped to ≥ 80 to give headroom. |
| `lock_macro` | (real) Per §5.10. |

### 5.13 Tests (03c-1)

- **Unit (`consensus`):** `book::micro_ring` push-and-drain; `proposer::round_robin` distinct primary/backup; `two_chain::newly_finalized_height` (genesis → None, h=1 → None, h=2 → Some(1), parent-mismatch → None); `lock_macro` collision via SM dispatch.
- **Unit (`consensus`):** `macro_qc::try_finalize_mode0` returns `None` below threshold, `Some` at exactly `2f + 1`, bitmap matches signer set.
- **Integration (`crates/consensus/tests/macro_fin_basic.rs`):** focused contract tests for the new public surface — `StateMachine::new(cfg, self_id)` compiles and round-trips; `ProposerSchedule::round_robin` matches the doc formula at heights 0..2n; `LockMacro::try_lock` returns `Err` on a same-height conflicting hash. Full E2E (8 wave commits → MacroProposal → 2f+1 BlsPartials → MacroQc → Justified → next-window → Finalized) is covered by `apps/sim happy_path` (§5.12). Building a 4-validator 8-wave DAG inside `consensus/tests` to repeat the E2E here would add ~200 lines of vertex-construction boilerplate for no extra coverage; the sim scenario already exercises the same code paths with real `bullshark` integration.
- **Sim scenario:** `happy_path` green with `safety_ok && liveness_ok && lock_macro_ok`; notes contain `"l3_finality_active"`.
- **Sim determinism:** `replay.rs` golden trace — same seed → same ordered list of `Action` discriminants per validator across two runs. Update goldens for L3.
- **CLI:** `cargo build -p cli`; `replay_log` accepts the new `StateMachine::new(cfg, ValidatorId::zero())` stub signature.

### 5.14 Acceptance criteria (03c-1)

- `cargo test -p consensus --locked` and `cargo test -p sim --locked` pass.
- `cargo build --workspace --locked` passes (node, cli, sim).
- `apps/sim happy_path` report: `safety_ok && liveness_ok && lock_macro_ok`; notes contain `"l3_finality_active"`; no notes reference `"lock_macro_skipped_until_03c"`.
- For default config (`W=8`, 4 validators, 80 rounds): ≥1 validator's `VirtualPersistence` records ≥1 `BlobStatus::Finalized` blob and ≥2 `MacroQc`s.
- `StateMachine::step` never returns `Err` on the honest factory inputs.
- No new `consensus` dependencies beyond existing workspace crates; no tokio / libp2p / rocksdb in `consensus`.

---

## 6. Phase 03c-2 — L3 full (outline)

Not landed in 03c-1; listed here for ordering only.

| Module | Deliverable |
|--------|-------------|
| `macro_fin/proposer.rs` | `ProposerSchedule::vrf_sortition(beacon, set, reputation, height)` via existing `leader::vrf_sortition` + `leader::reputation`. `vrf_proof` field is populated. |
| `macro_fin/timer.rs` (new) | `T_macropropose` timer: SM schedules a timer on candidate-build for non-proposers; if it fires without seeing `MacroProposalReceived`, the **backup** proposer emits. Uses the same `TimerScheduler` pattern as `WaveBook`. |
| `macro_fin/aggregation/mode_a_subnet.rs` | Real subnet rotation. `Event::SubnetAggregateReceived` real path; `Action::BroadcastSubnetAggregate` emitted by subnet aggregators. `select_mode` becomes real. |
| `macro_fin/aggregation/mode_b_leaderless.rs` | Real fallback when both primary and backup miss their slots. |
| `apps/sim/src/scenarios/` | `mode_b_fallback`, `byzantine_split`, `network_partition` extended with L3 assertions. May `#[ignore]` with issue link if flaky. |

Real BLS sign/verify deferred to **03d** (separate spec).

---

## 7. `node` and `cli` during Milestone B

- **03c-1:** `node` and `cli` compile against `StateMachine::new(cfg, self_id)`; the two call sites use their configured / startup-derived validator id. No new orchestrator behavior is required — the L3 actions (`BroadcastMacroProposal / BroadcastBlsPartial / BroadcastMacroQc / PersistMacroQc / UpdateBlobStatus`) are already plumbed through `apps/node/src/orchestrator.rs` via the same `Action` match the L2 work uses. Verification that production gossip carries the new topics is deferred to a small follow-up plan (`06b-l3`) after 03c-1 lands and `apps/sim` proves the algorithm.
- **03c-2:** `T_macropropose` lands in `apps/node/src/timer.rs` as an additional timer dispatch arm; sim already has `VirtualTimer`.

---

## 8. Error handling

- `step` returns `Err` only for invariant violations (unknown candidate at a height we just adopted, stake math underflow, internal duplicate transition). Wire decode errors stay in `net`.
- Sim: log and continue on single-validator `step` error in 03c-1; scenarios fail checkers if quorum/liveness not met.
- No panic in `step` on events from the honest factory (§5.5).
- `on_macro_proposal` drops (silently, no error) on: wrong proposer, parent mismatch, micro_root mismatch. These are honest-disagreement conditions in 03c-2 partition scenarios; surfacing them as errors would mask the real failure mode (timeout).

---

## 9. Testing strategy summary

| Level | 03c-1 | 03c-2 |
|-------|-------|-------|
| Unit | book ring, round-robin proposer, 2-chain advance, lock_macro via SM, macro_qc Mode 0 threshold | VRF sortition, Mode A subnet assign + aggregate, Mode B fallback, T_macropropose timer expiry |
| `consensus` integration | focused contract tests (`StateMachine::new(cfg, self_id)` round-trip, `ProposerSchedule::round_robin` doc formula, `LockMacro::try_lock` conflict) — full E2E delegated to `apps/sim happy_path` | wave commit under proposer absence; subnet aggregation across 8 subnets; Mode B activation |
| `sim` scenario | `happy_path` to Finalized | + `mode_b_fallback`, `byzantine_split`, `network_partition` (extended for L3) |
| `cli` | stub ctx compile + replay | optional port-snapshot replay |
| Property | optional | proptest where cheap |
| Golden | action-kind trace per seed (updated for L3) | regenerated when algorithm replaces 03c-1 hot paths |

---

## 10. Risks and mitigations

| Risk | Mitigation |
|------|------------|
| `StateMachine::new` signature break ripples to `node`, `sim`, `cli` | Single-commit migration: §3 enumerates the three call sites. |
| `micro_root` diverges between proposer and verifier under reordered local commits | 03c-1 honest factory delivers vertices synchronously to all validators in identical order; reorder risk only matters under partition (03c-2 scenario). Spec calls this out. |
| `MacroBook.partials` unbounded growth under adversary spam | Only `BlsPartial`s for `checkpoint_hash`es registered via `MacroProposalReceived` for a known height are inserted; unknown hashes are dropped. Bound at any time is `n × W`. |
| `Actions` SmallVec capacity (currently 16) overflows under L3 fanout | Per-step max in 03c-1: `CertifiedVertexReceived` → 2 (BroadcastMicroQc + BroadcastMacroProposal); `MacroProposalReceived` → 2 (BroadcastBlsPartial + UpdateBlobStatus(SoftConfirmed)); `BlsPartialReceived` → 5 (BroadcastMacroQc + PersistMacroCheckpoint + PersistMacroQc + UpdateBlobStatus(Justified) + optional UpdateBlobStatus(Finalized)). 16-cap holds comfortably; regression test pins it. |
| `BlobId` projection from `MacroCheckpoint.hash[..16]` is lossy | Documented 03c-1 placeholder; L1 lands real per-blob granularity. |
| Sim flake from per-recipient jitter on `BroadcastBlsPartial` | Deterministic-by-`(sender_idx, recipient_idx)` ordering rule in `virtual_net` (§5.11); replay test pins it. |
| `happy_path` rounds bumped to 80 slows CI | Acceptable; sim is in-process and fast. |
| Bullshark inner state assumes one `BroadcastMicroQc` per wave; macro_fin assumes one ring push per local emission | Single integration point: `macro_fin::on_local_micro_qcs` walks only `Action::BroadcastMicroQc` variants in the slice bullshark just produced. Other action kinds ignored. |
| Late-joining validator sees `MacroQcReceived` without having built the corresponding `candidate` | In 03c-1, drop with a debug log; macro_fin lacks a fast-sync path. 03c-2 / 03d adds checkpoint-sync. Spec note. |

---

## 11. Self-review

| Check | Result |
|-------|--------|
| Placeholders | None — all simplifications explicit. Round-robin formula, fixture sig derivations, `select_mode` not-called-in-hot-paths, `BlobId` projection, static `Epoch(0)`, `vrf_proof = ZERO_VRF`, `VoteRecord` zero source/target all noted. |
| Consistency | Data flow (§5.5), dispatch (§5.4), `MacroBook` (§5.3), per-file surface (§5.6) match. All `Action` variants handled by sim (§5.11). |
| Scope | 03c-1 boundary clean: backup proposer, Mode A/B, real BLS, surround detection, inactivity leak, light-client sync all named and deferred to 03c-2 / 03d / 06b-l3. |
| Ambiguity | Round-robin formula explicit. `micro_root` byte concat explicit. 2-chain definition explicit. `lock_macro` suppression vs evidence emission distinguished. Drop-vs-error policy in `on_macro_proposal` documented. |
| Migration | Single signature break (`StateMachine::new`); three call sites enumerated; no port surface change. |

---

## 12. Next step

1. User reviews this spec.
2. Implementation plan: `docs/superpowers/plans/2026-05-22-03c1-l3-minimal-sim.md`.
3. Execute 03c-1 first (`subagent-driven-development` or `executing-plans`).
4. 03c-2 spec + plan after 03c-1 lands.
