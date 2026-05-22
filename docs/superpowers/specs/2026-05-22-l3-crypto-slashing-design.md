# Design: L3 Crypto & Slashing (03d)

**Date:** 2026-05-22  
**Status:** Draft (plan-ready)  
**Audience:** Contributors replacing fixture BLS/VRF and wiring slash detectors  
**Prerequisite:** [03c-2 landed](../plans/2026-05-22-03c2-l3-full.md) — VRF-equivalent proposer, Modes A/B, sim scenarios green.

**Relations:** [`2026-05-22-l3-macro-finality-design.md`](2026-05-22-l3-macro-finality-design.md) §4 (03d row); [`2026-05-12-02-crypto-crate.md`](../plans/2026-05-12-02-crypto-crate.md); whitepaper §9 (aggregation), §13.5 (`lock_macro`), slashing penalties (Table 17.1 via `Config::slashing`).

---

## 1. Goals

Replace **deterministic fixture crypto** on the L3 hot path with **real BLS12-381 sign/verify and aggregate**, and turn **slash detectors** from stubs into SM-driven `Action::EmitSlashEvidence` emissions that sim persists and CLI can verify.

**In scope (03d):**

- Canonical message bytes + DST usage for macro proposal sig, validator partials, subnet aggregates, and `MacroQc` verification.
- `HostContext` gains a **`SignerPort`** (local validator only); sim derives keys from scenario seed; `ValidatorSet` carries real `bls_pubkey` bytes.
- Verify-on-receive: drop invalid `MacroProposal` / `BlsPartial` / `SubnetAggregate` / `MacroQc` (silent drop, same policy as wrong-proposer).
- `macro_qc` builds real `BlsAggSig` via `crypto::bls::aggregate_sigs` + `verify_aggregate` before adoption.
- **Detectors:** macro equivocation, Casper-FFG surround, double-vote; `verify_evidence` checks signatures and structural rules.
- Sim: wire `EmitSlashEvidence`, implement `adversary/byzantine.rs` + `equivocation_inject` scenario.
- **Macro proposer VRF:** populate `vrf_proof` with real `crypto::vrf::vrf_prove` / verify on receive (alpha = `BEACON ‖ height ‖ validator_id` bytes).

**Non-goals (03d):**

- On-chain slashing execution / stake deduction (penalty math in `slashing/penalty.rs` stays callable but SM does not apply economic state).
- Inactivity leak **emission** (config + `compute()` exist; wire in 03d+ or 06b).
- L2 micro QC real BLS (optional stretch — macro path is P0).
- Production node key management / HSM (**06b-l3**).
- L4 BTC anchor, DKG ceremony.

---

## 2. Architectural decisions

| Topic | Decision |
|-------|----------|
| Signing locus | **`SignerPort` on `HostContext`** — `consensus` stays free of persisted `SecretKey`; sim/node inject implementation per validator SM. |
| Message binding | Reuse existing DSTs in `crypto::hash::dst`: `MACRO_PROPOSER_SIG`, `VALIDATOR_BLS_PARTIAL`, `MACRO_CHECKPOINT` for QC body; payload = canonical concat (documented per type). |
| Fixture removal | Delete `book::fixture_*` from hot path; keep `fixture_bls_sig` only in tests/golden migration helpers until goldens refreshed. |
| Partial verify | `on_bls_partial`: verify sig under offender pubkey from valset; reject wrong subnet assignment (`assign.index_for(validator) != bp.subnet` in Mode A). |
| QC verify | Before `finish_macro_qc_adoption`, `verify_macro_qc(qc, set, checkpoint_hash)` — bitmap ↔ pubkeys, aggregate sig over `MACRO_CHECKPOINT` bytes. |
| FFG epochs | Map macro height → Casper epochs: `source = Epoch(h-1)`, `target = Epoch(h)` for vote at height `h > 0`; genesis votes use `0 → 0` until height ≥ 1 (documented). |
| Equivocation store | `MacroBook.proposals_seen: HashMap<(Height, ValidatorId), Vec<Hash32>>` — second distinct checkpoint hash at same pair → `MacroEquivocation` evidence. |
| VRF alpha | `alpha = beacon.0 \|\| height.to_be_bytes() \|\| validator_id` (match `macro_sortition_beta` input); proof from sim/node `VrfKey` derived like BLS. |
| Invalid crypto | **Drop** (empty `Actions`), increment optional `book.rejected_crypto` counter for tests — do not `Err` from `step` on wire attacks. |
| Determinism | Sim keys = `ChaCha20Rng::from_seed(hash(seed \|\| validator_index))` → `SecretKey::random`; goldens must be regenerated. |

---

## 3. Port & file map

| File | Responsibility |
|------|----------------|
| `crates/consensus/src/ports/signer.rs` | **NEW** `SignerPort` trait |
| `crates/consensus/src/host_context.rs` | Add `signer: &'a dyn SignerPort` |
| `crates/consensus/src/macro_fin/messages.rs` | **NEW** canonical encode for sign/verify |
| `crates/consensus/src/macro_fin/verify.rs` | **NEW** `verify_proposal`, `verify_partial`, `verify_subnet_agg`, `verify_macro_qc` |
| `crates/consensus/src/macro_fin/book.rs` | Remove fixture helpers from hot path; add `proposals_seen`, `rejected_crypto` |
| `crates/consensus/src/macro_fin/mod.rs` | Sign via `ctx.signer`; verify on all `on_*` receives; emit slash actions |
| `crates/consensus/src/macro_fin/macro_qc.rs` | Real aggregate in `try_finalize_mode0/a/b` |
| `crates/consensus/src/macro_fin/proposer.rs` | Real `vrf_proof` via `SignerPort::vrf_prove` or sub-trait |
| `crates/consensus/src/slashing/surround.rs` | Implement `scan_for_surround` |
| `crates/consensus/src/slashing/equivocation.rs` | Implement `verify` + `detect` |
| `crates/consensus/src/slashing/double_vote.rs` | **NEW** detector |
| `crates/consensus/src/slashing/evidence.rs` | Real `verify_evidence` dispatch |
| `apps/sim/src/keys.rs` | **NEW** deterministic `ValidatorKeyRing` |
| `apps/sim/src/world.rs` | Real pubkeys in valset; per-validator `SignerPort`; `EmitSlashEvidence` |
| `apps/sim/src/adversary/byzantine.rs` | `inject_equivocation` implementation |
| `apps/node/src/runtime.rs` | Stub `SignerPort` (dev keys from config path) — minimal, can error if no key |
| `crates/consensus/tests/macro_fin_bls.rs` | **NEW** sign/verify + aggregate threshold tests |
| `crates/consensus/tests/slashing_detect.rs` | **NEW** surround/equivocation unit tests |

---

## 4. Acceptance criteria

- `cargo test -p consensus -p crypto -p sim --locked` pass.
- `happy_path`, `mode_b_fallback`, `mode_a_subnet` remain green after golden refresh.
- `equivocation_inject` reports `safety_ok` may be false but **`EmitSlashEvidence` persisted** and `cli slashing verify` accepts evidence (or rejects tampered copy).
- No `fixture_bls_sig` on L3 hot path in `macro_fin` (grep check in CI note).
- `verify_evidence` returns `Err` on tampered `SlashEvidence` in unit tests.

**Next:** [06b-l3](2026-05-22-03c2-l3-full.md) node production gossip, or inactivity-leak emission follow-up.
