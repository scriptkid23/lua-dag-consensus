# L3 Crypto & Slashing (03d) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace fixture L3 cryptography with real BLS sign/verify/aggregate, real macro-proposer ECVRF proofs, and working slash detectors that emit persistable `SlashEvidence`.

**Architecture:** Add `SignerPort` to `HostContext` so `consensus` signs only via host-injected keys while verification uses `ValidatorSet` pubkeys + `crypto::bls`. Centralize message encoding in `macro_fin/messages.rs`. On every macro receive path, verify before mutating `MacroBook`; on successful votes, run detectors and push `Action::EmitSlashEvidence`. Sim derives deterministic key material from scenario seed and implements `inject_equivocation`.

**Tech Stack:** Rust 1.85, `blst` (existing), `crypto`, `consensus`, `types`, `sim`, `node`, `cli`. No new workspace dependencies.

**Spec:** [`docs/superpowers/specs/2026-05-22-l3-crypto-slashing-design.md`](../specs/2026-05-22-l3-crypto-slashing-design.md)

**Prerequisite:** 03c-2 complete (`happy_path`, `mode_b_fallback`, `mode_a_subnet` green).

---

## Design decisions (lock-in)

| Topic | Decision |
|-------|----------|
| Signing | `SignerPort::sign_bls(dst, msg)` + `SignerPort::vrf_prove(alpha)` on `HostContext` |
| Partial message | `dst::VALIDATOR_BLS_PARTIAL` + `validator_id ‖ checkpoint_hash` (same bytes as fixture) |
| Proposer message | `dst::MACRO_PROPOSER_SIG` + `proposer ‖ checkpoint.hash` |
| QC message | `dst::MACRO_CHECKPOINT` + canonical `MacroCheckpoint` borsh bytes |
| Subnet agg verify | Re-verify each partial bit in aggregate bitmap matches stored partial sigs (Mode A) OR treat agg as QC-over-subnet-signers with subnet-local message |
| Invalid sig | Drop event (empty `Actions`); do not panic in `step` |
| VRF | Replace VRF-equivalent `vrf_proof` fill with `crypto::vrf::vrf_prove`; verify on `on_macro_proposal` |
| Vote epochs | `source = Epoch(h.0.saturating_sub(1))`, `target = Epoch(h.0)` for height `h` |
| Inactivity leak | **Deferred** — leave `compute()` only; no `EmitSlashEvidence` yet |

---

## File map

| File | Action |
|------|--------|
| `crates/consensus/src/ports/signer.rs` | **CREATE** |
| `crates/consensus/src/ports/mod.rs` | export `SignerPort` |
| `crates/consensus/src/host_context.rs` | add `signer` field |
| `crates/consensus/src/macro_fin/messages.rs` | **CREATE** encode helpers |
| `crates/consensus/src/macro_fin/verify.rs` | **CREATE** verify helpers |
| `crates/consensus/src/macro_fin/book.rs` | `proposals_seen`, drop fixture from hot path |
| `crates/consensus/src/macro_fin/macro_qc.rs` | real `aggregate_sigs` |
| `crates/consensus/src/macro_fin/proposer.rs` | vrf prove/verify hooks |
| `crates/consensus/src/macro_fin/mod.rs` | sign/verify/slash integration |
| `crates/consensus/src/slashing/*.rs` | implement detectors + evidence verify |
| `crates/consensus/src/state_machine.rs` | pass `signer` into `HostContext` construction sites in tests |
| `apps/sim/src/keys.rs` | **CREATE** `ValidatorKeyRing` |
| `apps/sim/src/world.rs` | key ring + real pubkeys + `EmitSlashEvidence` |
| `apps/sim/src/adversary/byzantine.rs` | real `inject_equivocation` |
| `apps/sim/src/scenarios/equivocation_inject.rs` | wire adversary |
| `apps/node/src/signer.rs` | **CREATE** file-based or env stub |
| `apps/cli` | no change required (verify already calls `verify_evidence`) |
| `crates/consensus/tests/macro_fin_bls.rs` | **CREATE** |
| `crates/consensus/tests/slashing_detect.rs` | **CREATE** |

---

### Task 1: `SignerPort` + `HostContext`

**Files:**
- Create: `crates/consensus/src/ports/signer.rs`
- Modify: `crates/consensus/src/ports/mod.rs`, `host_context.rs`

- [ ] **Step 1: Add trait**

```rust
//! Local validator signing for one `StateMachine::step` call.
use types::crypto_types::{BlsSig, VrfProof};
use crate::error::Result;

/// Signs on behalf of the validator that owns this `StateMachine`.
pub trait SignerPort {
    /// BLS sign under the local validator key.
    fn sign_bls(&self, dst: &[u8], msg: &[u8]) -> BlsSig;
    /// ECVRF prove for macro/L2 sortition alphas.
    fn vrf_prove(&self, alpha: &[u8]) -> Result<(VrfProof, types::crypto_types::Hash32)>;
}
```

- [ ] **Step 2: Extend `HostContext`**

```rust
pub struct HostContext<'a> {
    // ... existing fields ...
    pub signer: &'a dyn SignerPort,
}
```

- [ ] **Step 3: Fix compile** — add `signer` to every `HostContext { ... }` literal in `consensus` tests, `sim`, `node`, `cli` (stub `PanickingSigner` in tests first).

- [ ] **Step 4: Run**

```bash
cargo build -p consensus -p sim -p node --locked
```

- [ ] **Step 5: Commit**

```bash
git add crates/consensus/src/ports/ crates/consensus/src/host_context.rs
git commit -m "feat(consensus): SignerPort on HostContext (03d)"
```

---

### Task 2: Canonical macro message bytes

**Files:**
- Create: `crates/consensus/src/macro_fin/messages.rs`
- Modify: `crates/consensus/src/macro_fin/mod.rs` (pub mod messages)

- [ ] **Step 1: Failing tests** in `messages.rs`:

```rust
#[test]
fn partial_message_is_stable() {
    let v = ValidatorId([1; 32]);
    let h = Hash32([2; 32]);
    assert_eq!(partial_message(&v, &h), partial_message(&v, &h));
}
```

- [ ] **Step 2: Implement**

```rust
pub fn partial_message(validator: &ValidatorId, checkpoint_hash: &Hash32) -> Vec<u8> {
    let mut m = Vec::with_capacity(64);
    m.extend_from_slice(validator.as_bytes());
    m.extend_from_slice(&checkpoint_hash.0);
    m
}

pub fn proposer_message(proposer: &ValidatorId, checkpoint: &MacroCheckpoint) -> Vec<u8> {
    let mut m = Vec::with_capacity(64);
    m.extend_from_slice(proposer.as_bytes());
    m.extend_from_slice(&checkpoint.hash.0);
    m
}

pub fn checkpoint_message(cp: &MacroCheckpoint) -> Vec<u8> {
    borsh::to_vec(cp).expect("MacroCheckpoint must borsh-encode")
}
```

- [ ] **Step 3: Run**

```bash
cargo test -p consensus macro_fin::messages --locked
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(consensus): canonical L3 BLS message bytes (03d)"
```

---

### Task 3: `macro_fin/verify.rs`

**Files:**
- Create: `crates/consensus/src/macro_fin/verify.rs`

- [ ] **Step 1: Tests** — valid sig passes, flipped byte fails.

- [ ] **Step 2: Implement helpers using `crypto::bls::sign::verify`**

```rust
pub fn verify_partial(
    set: &ValidatorSet,
    bp: &BlsPartial,
) -> bool {
    let Some(pk) = set.entries.iter().find(|e| e.id == bp.validator) else {
        return false;
    };
    let msg = messages::partial_message(&bp.validator, &bp.checkpoint_hash);
    crypto::bls::sign::verify(&pk_from_entry(pk), dst::VALIDATOR_BLS_PARTIAL, &msg, &bp.sig).is_ok()
}
```

(Map `BlsPubkey` → `crypto::bls::PublicKey` via `PublicKey::from_bytes` helper in crypto or consensus adapter.)

- [ ] **Step 3: `verify_macro_qc`** — extract pubkeys from bitmap in validator-set order; `verify_aggregate` over `checkpoint_message`.

- [ ] **Step 4: `verify_proposal`** — proposer sig + optional `vrf_verify` on alpha.

- [ ] **Step 5: Run tests, commit**

```bash
cargo test -p consensus macro_fin::verify --locked
git commit -m "feat(consensus): L3 BLS verify helpers (03d)"
```

---

### Task 4: Sim deterministic key ring

**Files:**
- Create: `apps/sim/src/keys.rs`
- Modify: `apps/sim/src/lib.rs`, `world.rs`

- [ ] **Step 1: `ValidatorKeyRing`**

```rust
pub struct ValidatorKeyRing {
    bls: Vec<crypto::bls::SecretKey>,
    vrf: Vec<crypto::vrf::VrfKey>,
}

impl ValidatorKeyRing {
    pub fn from_seed(seed: [u8; 32], n: u32) -> Self { /* ChaCha20 per index */ }
}

impl consensus::ports::SignerPort for ValidatorSigner<'_> { /* one validator view */ }
```

- [ ] **Step 2: `World::new`** — fill `ValidatorEntry.bls_pubkey` from ring; store `key_rings: Vec<ValidatorSigner>` parallel to `machines`.

- [ ] **Step 3: `step_validator`** — pass `signer: &key_rings[idx]` into `HostContext`.

- [ ] **Step 4: Run**

```bash
cargo test -p sim --locked
```

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(sim): deterministic BLS/VRF key ring (03d)"
```

---

### Task 5: Replace fixture signing on emit paths

**Files:**
- Modify: `crates/consensus/src/macro_fin/mod.rs`, `book.rs`, `macro_qc.rs`

- [ ] **Step 1: `on_macro_proposal` emit** — after lock:

```rust
let msg = messages::proposer_message(&book.self_id, &p.checkpoint);
let proposer_sig = ctx.signer.sign_bls(dst::MACRO_PROPOSER_SIG, &msg);
let (vrf_proof, _) = ctx.signer.vrf_prove(&vrf_alpha(&beacon, height, &book.self_id))?;
```

- [ ] **Step 2: `BroadcastBlsPartial`** — `ctx.signer.sign_bls(dst::VALIDATOR_BLS_PARTIAL, &partial_message(...))`.

- [ ] **Step 3: `macro_qc::try_finalize_mode0`** — collect `BlsSig` from partials map (store sigs in `MacroBook` new field `partial_sigs: HashMap<(Hash32, ValidatorId), BlsSig>` on insert); `aggregate_sigs` → real agg bytes.

- [ ] **Step 4: Remove** `[0xCD; 96]` / `[0xDE; 96]` constants from finalize paths.

- [ ] **Step 5: Verify-on-receive** at top of `on_macro_proposal`, `on_bls_partial`, `on_subnet_aggregate`, `on_macro_qc_received`:

```rust
if !verify::verify_partial(&set, &bp) {
    book.rejected_crypto += 1;
    return Ok(Actions::new());
}
```

- [ ] **Step 6: Run + refresh goldens**

```bash
cargo test -p sim happy_path_runs_and_replays_bit_identical --locked
# update replay golden if discriminant trace unchanged but payload bytes differ
```

- [ ] **Step 7: Commit**

```bash
git commit -m "feat(consensus): real BLS sign/verify on L3 hot path (03d)"
```

---

### Task 6: Real ECVRF macro proposer proof

**Files:**
- Modify: `crates/consensus/src/macro_fin/proposer.rs`, `mod.rs`

- [ ] **Step 1: `vrf_alpha(beacon, height, validator)`** shared helper (same bytes as `macro_sortition_beta`).

- [ ] **Step 2: Primary emit in `on_local_micro_qcs`** — `vrf_proof` from `ctx.signer.vrf_prove(&alpha)`.

- [ ] **Step 3: `on_macro_proposal`** — `crypto::vrf::vrf_verify(pk, alpha, &p.vrf_proof)` for `p.proposer`; drop on failure.

- [ ] **Step 4: Tests in `macro_fin_vrf.rs`** — extend with prove/verify round-trip using `ValidatorKeyRing` test helper.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(consensus): real ECVRF macro proposer proofs (03d)"
```

---

### Task 7: Vote epochs + surround / double-vote detectors

**Files:**
- Modify: `crates/consensus/src/macro_fin/mod.rs` (vote record)
- Modify: `crates/consensus/src/slashing/surround.rs`
- Create: `crates/consensus/src/slashing/double_vote.rs`
- Modify: `crates/consensus/src/slashing/mod.rs`

- [ ] **Step 1: Vote recording**

```rust
VoteRecord {
    source: Epoch(height.0.saturating_sub(1)),
    target: Epoch(height.0),
    checkpoint: p.checkpoint.hash,
}
```

Sign vote with `dst::MACRO_VOTE` + borsh vote payload (add `messages::vote_message`).

- [ ] **Step 2: `scan_for_surround`** — for each new vote, compare against `vote_book.votes_of(validator)`; if ∃ b: b.source < a.source ≤ b.target < a.target, build `SurroundVote` with stored sigs.

- [ ] **Step 3: `scan_for_double_vote`** — same `target`, different `checkpoint`.

- [ ] **Step 4: After `vote_book.record` in `on_macro_proposal`**, if detector returns evidence → `actions.push(Action::EmitSlashEvidence(...))`.

- [ ] **Step 5: Unit tests** `crates/consensus/tests/slashing_detect.rs`.

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(consensus): surround + double-vote slash detectors (03d)"
```

---

### Task 8: Macro equivocation detector

**Files:**
- Modify: `crates/consensus/src/macro_fin/book.rs`, `mod.rs`, `slashing/equivocation.rs`

- [ ] **Step 1: `proposals_seen: HashMap<(Height, ValidatorId), Vec<Hash32>>`**

- [ ] **Step 2: On `on_macro_proposal`**, if second distinct hash for same `(height, proposer)` → build `MacroEquivocation { a, b }` with both proposals + sigs, emit slash, drop second proposal.

- [ ] **Step 3: `equivocation::verify`** — verify both proposer sigs under offender pubkey.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(consensus): macro equivocation detector (03d)"
```

---

### Task 9: `verify_evidence` + sim `EmitSlashEvidence`

**Files:**
- Modify: `crates/consensus/src/slashing/evidence.rs`
- Modify: `apps/sim/src/world.rs`, `virtual_persistence.rs`

- [ ] **Step 1: Dispatch verify**

```rust
pub fn verify_evidence(ev: &SlashEvidence) -> Result<()> {
    match ev {
        SlashEvidence::MacroEquivocation(e) => equivocation::verify(e),
        SlashEvidence::Surround(e) => surround::verify(e),
        SlashEvidence::DoubleVote(e) => double_vote::verify(e),
    }
}
```

- [ ] **Step 2: Sim `apply_actions`**

```rust
Action::EmitSlashEvidence(ev) => {
    self.persistence[idx].append_slash_evidence(&ev)?;
    self.net.enqueue_from_action(...); // optional gossip
}
```

Remove `debug_assert!(false, "slashing → 03d")`.

- [ ] **Step 3: `cargo test -p cli`** slashing verify subcommand with generated evidence fixture.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat: verify_evidence + sim slash persistence (03d)"
```

---

### Task 10: Adversary + `equivocation_inject` scenario

**Files:**
- Modify: `apps/sim/src/adversary/byzantine.rs`
- Modify: `apps/sim/src/scenarios/equivocation_inject.rs`

- [ ] **Step 1: Implement `inject_equivocation`**

Build two `MacroCheckpoint` with same `height` / `proposer` but different `micro_root` → two `MacroProposal` with valid sigs from offender key → enqueue to disjoint partition halves via `world.set_partition` + targeted `step_validator`.

- [ ] **Step 2: Scenario**

```rust
world.set_partition((0..n/2), (n/2..n));
inject_equivocation(&mut world, offender_idx);
world.run(rounds);
// expect: slash evidence stored; safety may fail; note "slash_emitted"
```

- [ ] **Step 3: Optional integration test** `apps/sim/tests/equivocation_inject.rs`.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(sim): equivocation inject adversary (03d)"
```

---

### Task 11: Node signer stub

**Files:**
- Create: `apps/node/src/signer.rs`
- Modify: `apps/node/src/runtime.rs`, `lib.rs`

- [ ] **Step 1: Load key from path in `Config` or env `LUA_DAG_BLS_KEY` (document dev-only).

- [ ] **Step 2: Wire `HostContext.signer` in orchestrator step loop.

- [ ] **Step 3: `cargo build -p node --locked`** (no live network required).

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(node): dev SignerPort stub for L3 (03d)"
```

---

### Task 12: Integration tests + acceptance

- [ ] **Step 1: `crates/consensus/tests/macro_fin_bls.rs`**

End-to-end: 4 validators, threshold partials, real aggregate, `verify_macro_qc` ok.

- [ ] **Step 2: Scenario matrix**

```bash
cargo test -p consensus -p sim --locked
cargo run -p sim --release --locked -- --scenario happy-path --validators 4 --rounds 96 --seed 0x01
cargo run -p sim --release --locked -- --scenario mode-b-fallback --validators 4 --rounds 128 --seed 2
cargo run -p sim --release --locked -- --scenario mode-a-subnet --validators 8 --rounds 128 --seed 3
cargo run -p sim --release --locked -- --scenario equivocation-inject --validators 4 --rounds 96 --seed 4
```

- [ ] **Step 3: Update parent spec** [`2026-05-22-l3-macro-finality-design.md`](../specs/2026-05-22-l3-macro-finality-design.md) — Status: `03c-2 landed; 03d plan ready`; add §4 table row linking this plan.

- [ ] **Step 4: Grep guard (manual CI note)**

```bash
rg "fixture_bls_sig|0xCD; 96" crates/consensus/src/macro_fin --glob '!**/tests/**'
# expect: no matches on hot path
```

---

## Done — 03d acceptance criteria

- L3 hot path uses real BLS sign/verify and aggregate QC signatures.
- Macro proposer `vrf_proof` is real ECVRF (prove + verify).
- Surround, double-vote, macro-equivocation detectors emit `SlashEvidence`; `verify_evidence` validates structure + sigs.
- Sim persists slash evidence; `equivocation_inject` exercises adversary path.
- Existing 03c-2 scenarios stay green after golden refresh.
- Inactivity leak emission and on-chain penalty application remain **out of scope**.

**Next:** **06b-l3** (node production L3 gossip/RPC + live signer) or **03d+** inactivity leak emission.
