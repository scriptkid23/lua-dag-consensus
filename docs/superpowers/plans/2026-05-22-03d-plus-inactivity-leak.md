# Inactivity Leak Emission (03d+) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the existing `slashing::inactivity_leak::compute()` helper into macro-finality state so consecutive **unfinalized macro windows** increment a counter and, once ≥ `inactivity_leak_trigger_windows` (default 4), the SM emits a host-visible **`Action::NotifyInactivityLeak`** for metrics / future stake accounting — without on-chain execution.

**Architecture:** Extend `MacroBook` with `unfinalized_windows: u32` updated on every `two_chain` adoption: reset to 0 when a height becomes `Finalized`, else increment when a window closes with only `Justified`. On threshold crossing, emit `NotifyInactivityLeak { windows, bps_per_window }` **once per streak** (dedupe flag). Sim checker observes the action; node applier (06b-l3) logs metric. **No new `SlashEvidence` variant** — inactivity leak is economic (Table 17.1 bps), not a signed offence bundle.

**Tech Stack:** Rust 1.85, `consensus`, `sim`, optional `apps/node` metric hook.

**Spec:** [`2026-05-22-l3-crypto-slashing-design.md`](../specs/2026-05-22-l3-crypto-slashing-design.md) §1 non-goals (emission deferred); penalty rate in `Config::macro_fin`.

**Prerequisite:** **03d** complete (real macro windows finalize in sim happy-path).

**Depends on (optional):** **06b-l3** Task 1 if node should record metrics — sim-only path can land first.

---

## Design decisions (lock-in)

| Topic | Decision |
|-------|----------|
| Evidence type | **Not** `SlashEvidence` — use new host action `NotifyInactivityLeak` |
| Window definition | One macro height adopted (`Justified`) without advancing `two_chain.finalized_head` counts as +1 unfinalized window |
| Reset | Any `Finalized` adoption resets counter to 0 and clears dedupe |
| Rate | `bps = cfg.macro_fin.inactivity_leak_bps_per_window` (50 = 0.5 %) when `compute()` returns `should_apply == true` |
| Re-emit | At most one notification per unfinalized streak until reset |
| On-chain slash | Out of scope — `penalty::PenaltyKind::InactivityLeak` remains callable by future executor |

---

## File map

| File | Action |
|------|--------|
| `crates/consensus/src/action.rs` | add `NotifyInactivityLeak { windows, bps }` |
| `crates/consensus/src/macro_fin/book.rs` | `unfinalized_windows`, `leak_notified` |
| `crates/consensus/src/macro_fin/mod.rs` | update counter in `finish_macro_qc_adoption` / `on_macro_qc_received` |
| `crates/consensus/src/slashing/inactivity_leak.rs` | export helper used by mod (already exists) |
| `apps/sim/src/world.rs` | handle action (metric counter / persistence stub) |
| `apps/sim/src/scenarios/inactivity_leak.rs` | **CREATE** adversarial liveness stall scenario |
| `apps/sim/tests/inactivity_leak.rs` | **CREATE** integration test |
| `apps/node/src/action_applier.rs` | increment prometheus counter (after 06b-l3 lands) |

---

### Task 1: `Action::NotifyInactivityLeak`

**Files:**
- Modify: `crates/consensus/src/action.rs`, `crates/consensus/tests/step_signature.rs`

- [ ] **Step 1: Add variant**

```rust
    /// Host notification: inactivity leak rate applies (not slash evidence).
    NotifyInactivityLeak {
        /// Consecutive unfinalized macro windows observed locally.
        windows: u32,
        /// Basis-points penalty rate for this window (Table 17.1).
        bps_per_window: u32,
    },
```

- [ ] **Step 2: Extend `step_signature` total test** to include new variant in round-trip list.

- [ ] **Step 3: Run**

```bash
cargo test -p consensus step_signature action::tests --locked
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(consensus): NotifyInactivityLeak action (03d+)"
```

---

### Task 2: `MacroBook` counter

**Files:**
- Modify: `crates/consensus/src/macro_fin/book.rs`, `mod.rs`

- [ ] **Step 1: Fields**

```rust
pub(crate) unfinalized_windows: u32,
pub(crate) leak_notified: bool,
```

- [ ] **Step 2: Helper**

```rust
fn note_macro_adoption(book: &mut MacroBook, cfg: &Config, actions: &mut Actions, finalized: bool) {
    if finalized {
        book.unfinalized_windows = 0;
        book.leak_notified = false;
        return;
    }
    book.unfinalized_windows = book.unfinalized_windows.saturating_add(1);
    let (bps, apply) = inactivity_leak::compute(cfg, book.unfinalized_windows);
    if apply && !book.leak_notified {
        book.leak_notified = true;
        actions.push(Action::NotifyInactivityLeak {
            windows: book.unfinalized_windows,
            bps_per_window: bps,
        });
    }
}
```

Call from `finish_macro_qc_adoption` after `two_chain.adopt`: pass `book.two_chain.newly_finalized_height().is_some()` as `finalized` for the **previous** height case, else `false` when only justified.

- [ ] **Step 3: Unit test** in `macro_fin/mod.rs`:

```rust
#[test]
fn inactivity_leak_emits_after_four_unfinalized_windows() {
    // adopt 4 macro QCs with parent chain broken OR same-height stall pattern
    // assert exactly one NotifyInactivityLeak with windows >= 4
}
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(consensus): inactivity leak counter + emit (03d+)"
```

---

### Task 3: Sim wiring

**Files:**
- Modify: `apps/sim/src/world.rs`, `apps/sim/src/scenarios/mod.rs`

- [ ] **Step 1: Handle action**

```rust
Action::NotifyInactivityLeak { windows, bps_per_window } => {
    self.metrics.inactivity_leak_emitted.inc();
    self.persistence[0].note_inactivity_leak(windows, bps_per_window); // optional stub
}
```

- [ ] **Step 2: Scenario `inactivity_leak`** — suppress finality (e.g. partition + no parent chain) for ≥4 macro heights while micro commits continue; expect notification in report notes.

- [ ] **Step 3: Integration test** `apps/sim/tests/inactivity_leak.rs`:

```bash
cargo test -p sim inactivity_leak --locked
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(sim): inactivity leak scenario + action handler (03d+)"
```

---

### Task 4: Acceptance

- [ ] **Step 1: Regression**

```bash
cargo test -p consensus -p sim --locked
cargo run -p sim --release --locked -- --scenario happy-path --validators 4 --rounds 96 --seed 0x01
# happy-path must NOT emit leak (finalizes within 4 windows)
```

- [ ] **Step 2: Update spec** — `2026-05-22-l3-crypto-slashing-design.md` §1: move inactivity emission from non-goals to "landed in 03d+".

- [ ] **Step 3: Commit**

```bash
git commit -m "docs: mark inactivity leak emission landed (03d+)"
```

---

## Done — 03d+ acceptance criteria

- After 4 consecutive macro windows without local `Finalized`, SM emits exactly one `NotifyInactivityLeak`.
- Finalization resets counter; happy-path sim never emits leak.
- `inactivity_leak` scenario demonstrates emission in notes/metrics.
- No change to BLS / slash evidence paths from 03d.

**Next:** stake executor applying `penalty::compute(InactivityLeak)` on-chain (future economics plan).
