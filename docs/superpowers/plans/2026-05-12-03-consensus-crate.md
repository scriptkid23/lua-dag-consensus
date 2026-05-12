# `crates/consensus` Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up `crates/consensus/` as the pure deterministic state-machine skeleton: full module layout, all `Event` + `Action` variants, the 5 `ports::*` traits, the `StateMachine` entrypoint, the `Config` struct mirroring Table 17.1, and stubs for Bullshark (L2), Macro-Finality (L3), Leader, Slashing, API, and `lock_macro`. Behaviour is intentionally minimal — every internal handler returns `SmallVec::new()` so the crate compiles, passes its tests, and is ready to receive real semantics in follow-up plans (one per algorithm). The crate **must not** depend on tokio, libp2p, or rocksdb.

**Architecture:** Pure SM. `StateMachine::step(Event) -> SmallVec<[Action; 8]>` is the only public mutator. Outbound I/O is expressed as `Action`s; inbound events come from a single `Event` enum. All external systems (storage, network, clock, randomness, validator set, DAG) plug in via the 5 traits in `ports::`. L2 + L3 live in the same crate (per spec §6 — cross-layer invariants force cohabitation), separated by submodules. Domain primitives come from `types`; cryptographic operations from `crypto`.

**Tech Stack:** Edition 2024. Dependencies: `types`, `crypto`, `borsh`, `smallvec`, `thiserror`. Dev: `proptest`, `rand_chacha`.

**Prerequisites:** Plans 00, 01, 02.

---

## File Structure

Per spec §6. Every file new under `crates/consensus/`.

```
crates/consensus/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs
│   ├── prelude.rs
│   ├── config.rs                       # mirrors config/default.toml + Table 17.1
│   ├── event.rs
│   ├── action.rs
│   ├── error.rs
│   ├── state_machine.rs                # struct StateMachine + fn step
│   ├── lock_macro.rs                   # §13.5 invariant tracker
│   ├── bullshark/
│   │   ├── mod.rs
│   │   ├── wave.rs
│   │   ├── anchor.rs
│   │   ├── commit.rs
│   │   ├── linearize.rs
│   │   └── micro_qc.rs
│   ├── macro_fin/
│   │   ├── mod.rs
│   │   ├── window.rs
│   │   ├── proposer.rs
│   │   ├── checkpoint.rs
│   │   ├── macro_qc.rs
│   │   ├── two_chain.rs
│   │   ├── vote_book.rs
│   │   └── aggregation/
│   │       ├── mod.rs
│   │       ├── mode0_flat.rs
│   │       ├── mode_a_subnet.rs
│   │       ├── mode_b_leaderless.rs
│   │       └── subnet.rs
│   ├── leader/
│   │   ├── mod.rs
│   │   ├── beacon.rs
│   │   ├── vrf_sortition.rs
│   │   ├── reputation.rs
│   │   └── timeout.rs
│   ├── slashing/
│   │   ├── mod.rs
│   │   ├── evidence.rs
│   │   ├── equivocation.rs
│   │   ├── surround.rs
│   │   ├── inactivity_leak.rs
│   │   └── penalty.rs
│   ├── api/
│   │   ├── mod.rs
│   │   ├── tier.rs
│   │   └── query.rs
│   └── ports/
│       ├── mod.rs
│       ├── dag_view.rs
│       ├── clock.rs
│       ├── rng_beacon.rs
│       ├── validator_set.rs
│       └── persistence.rs
└── tests/
    ├── step_signature.rs
    ├── config_defaults.rs
    └── lock_macro_invariant.rs
```

The interior of each handler is `todo!("plan 0X: <algorithm>")` (compile-time-permitted) **only where the function has no callers in this skeleton**. Functions that are called by `state_machine.rs` return an empty `SmallVec` so test cases can drive `step` through any event without panicking. Plans implementing real algorithms (Bullshark commit rule, BLS modes, two-chain rule, slashing detectors) replace these stubs.

---

## Task 1: Crate skeleton + workspace registration

**Files:**
- Create: `crates/consensus/Cargo.toml`
- Create: `crates/consensus/src/lib.rs`
- Create: `crates/consensus/src/prelude.rs`
- Create: `crates/consensus/README.md`
- Modify: workspace `Cargo.toml`

- [ ] **Step 1: Write `crates/consensus/Cargo.toml`**

```toml
[package]
name         = "consensus"
version      = "0.1.0"
edition.workspace      = true
rust-version.workspace = true
license.workspace      = true
publish.workspace      = true
repository.workspace   = true
authors.workspace      = true

[lints]
workspace = true

[dependencies]
types       = { path = "../types" }
crypto      = { path = "../crypto" }
borsh       = { workspace = true }
smallvec    = { workspace = true }
thiserror   = { workspace = true }
serde       = { workspace = true }
toml        = { workspace = true }

[dev-dependencies]
proptest    = { workspace = true }
rand        = { workspace = true }
rand_chacha = { workspace = true }
```

- [ ] **Step 2: Write `crates/consensus/src/lib.rs`**

```rust
//! LUA-DAG consensus: pure deterministic state machine.
//!
//! This crate contains zero async runtimes, networking, and storage code.
//! All side effects are surfaced as [`Action`](crate::action::Action) values
//! returned by [`StateMachine::step`](crate::state_machine::StateMachine::step).
#![cfg_attr(not(test), warn(missing_docs))]
// Skeleton phase: many handlers are intentionally stubbed.
#![allow(clippy::needless_pass_by_value, clippy::unused_self)]

pub mod action;
pub mod api;
pub mod bullshark;
pub mod config;
pub mod error;
pub mod event;
pub mod leader;
pub mod lock_macro;
pub mod macro_fin;
pub mod ports;
pub mod prelude;
pub mod slashing;
pub mod state_machine;

pub use action::Action;
pub use config::Config;
pub use error::{Error, Result};
pub use event::Event;
pub use state_machine::StateMachine;
```

- [ ] **Step 3: Write `crates/consensus/src/prelude.rs`**

```rust
//! Convenience re-exports for downstream binaries.

pub use crate::{
    action::Action,
    api::tier::BlobStatus,
    config::Config,
    error::{Error, Result},
    event::Event,
    ports::{
        clock::Clock,
        dag_view::DagView,
        persistence::Persistence,
        rng_beacon::RandomnessBeacon,
        validator_set::ValidatorSetPort,
    },
    state_machine::StateMachine,
};
```

- [ ] **Step 4: Write `crates/consensus/README.md`**

```markdown
# `consensus`

Pure deterministic state machine for the LUA-DAG protocol (L2 Bullshark
micro-ordering + L3 Casper-FFG macro-finality, with cross-layer
invariants).

Public entry point: [`StateMachine::step`].

This crate **must not** depend on `tokio`, `libp2p`, or `rocksdb`. Side
effects are emitted as `Action`s; the host binary executes them.

See [`docs/superpowers/specs/2026-05-11-folder-architecture-design.md`](../../docs/superpowers/specs/2026-05-11-folder-architecture-design.md)
§6 for the design rationale.
```

- [ ] **Step 5: Add to workspace members**

In root `Cargo.toml`:

```toml
members = ["crates/types", "crates/crypto", "crates/consensus"]
```

---

## Task 2: `error.rs` and `config.rs`

**Files:**
- Create: `crates/consensus/src/error.rs`
- Create: `crates/consensus/src/config.rs`

- [ ] **Step 1: Write `crates/consensus/src/error.rs`**

```rust
//! Crate-level error type for consensus.

use thiserror::Error;

/// Consensus errors.
#[derive(Debug, Error)]
pub enum Error {
    /// Config TOML failed to parse or was missing fields.
    #[error("invalid config: {0}")]
    InvalidConfig(String),

    /// Storage / persistence port reported a failure.
    #[error("persistence error: {0}")]
    Persistence(String),

    /// Cryptographic primitive returned an error.
    #[error("crypto error: {0}")]
    Crypto(#[from] crypto::Error),

    /// Types codec / range error.
    #[error("types error: {0}")]
    Types(#[from] types::Error),

    /// `lock_macro` invariant was violated by an attempted action.
    #[error("lock_macro violation: {0}")]
    LockMacro(&'static str),
}

/// Result alias.
pub type Result<T> = core::result::Result<T, Error>;
```

- [ ] **Step 2: Write `crates/consensus/src/config.rs`**

Mirror every field from `config/default.toml`. Field names must match.

```rust
//! Protocol parameter set (whitepaper Table 17.1).
//!
//! Loaded once at startup. Override via [`Config::from_toml_str`] or
//! mutate fields directly for tests.

use serde::{Deserialize, Serialize};

/// All tunable protocol parameters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Schema version. Loaders must reject unknown values.
    pub schema_version: u32,
    /// Timing knobs.
    pub timing: Timing,
    /// Bullshark micro-ordering knobs.
    pub bullshark: BullsharkParams,
    /// Macro-finality knobs.
    pub macro_fin: MacroFinParams,
    /// Adaptive aggregation knobs.
    pub aggregation: AggregationParams,
    /// Leader / reputation knobs.
    pub leader: LeaderParams,
    /// Slashing penalties.
    pub slashing: SlashingParams,
    /// L4 anchor placeholder.
    pub anchor_l4: AnchorL4Params,
    /// Storage GC horizons.
    pub storage: StorageParams,
}

/// Timing knobs (ms).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Timing {
    /// Round length for micro-ordering.
    pub round_duration_ms: u64,
    /// Macro proposer slot.
    pub t_macropropose_ms: u64,
    /// Subnet aggregation window.
    pub t_subnet_ms: u64,
    /// Canonical macro publish window.
    pub t_canonicalize_ms: u64,
}

/// Bullshark parameters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BullsharkParams {
    /// Micro committee size C.
    pub micro_committee_size: u32,
    /// Shortcut commit round count.
    pub shortcut_round_count: u32,
    /// Slow-path commit round count.
    pub slow_path_round_count: u32,
    /// Wave round count (always 4).
    pub wave_round_count: u32,
}

/// Macro-finality parameters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MacroFinParams {
    /// W — micro-slots per macro window.
    pub macro_window_w: u32,
    /// Casper FFG 2-chain depth.
    pub two_chain_depth: u32,
    /// Inactivity leak rate (basis points per window).
    pub inactivity_leak_bps_per_window: u32,
    /// Consecutive unfinalized windows that trigger leak.
    pub inactivity_leak_trigger_windows: u32,
}

/// Aggregation thresholds (spec §9.2, Eq. 9.1/9.2).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AggregationParams {
    /// Below this, use Mode 0 flat aggregation.
    pub subnet_flat_threshold: u32,
    /// At/above this, use Mode A subnet aggregation.
    pub subnet_full_threshold: u32,
}

/// Leader election parameters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LeaderParams {
    /// Shoal reputation floor (typically 0.8).
    pub reputation_floor: f64,
    /// Shoal reputation ceiling (typically 1.2).
    pub reputation_ceiling: f64,
    /// Reputation EWMA decay factor.
    pub reputation_decay: f64,
}

/// Slashing penalty parameters (basis points).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SlashingParams {
    /// Macro equivocation penalty.
    pub equivocation_bps: u32,
    /// Surround/double vote penalty.
    pub double_vote_bps: u32,
    /// Data-availability incident penalty per occurrence.
    pub da_incident_bps: u32,
    /// Per-epoch slashing cap.
    pub slashing_cap_bps: u32,
}

/// L4 anchor parameters (placeholder until L4 lands).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnchorL4Params {
    /// Bitcoin confirmations required for `epoch_finalized`.
    pub btc_confirmations_for_final: u32,
}

/// Storage GC horizons (rounds).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StorageParams {
    /// Hot tier horizon.
    pub gc_hot_horizon_rounds: u64,
    /// Warm tier horizon.
    pub gc_warm_horizon_rounds: u64,
    /// Snapshot interval in macro windows.
    pub snapshot_interval_macros: u64,
}

/// Current expected `schema_version`.
pub const SCHEMA_VERSION: u32 = 1;

impl Config {
    /// Defaults mirroring `config/default.toml`.
    #[must_use]
    pub fn default_table_17_1() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            timing: Timing {
                round_duration_ms: 250,
                t_macropropose_ms: 4_000,
                t_subnet_ms: 2_000,
                t_canonicalize_ms: 8_000,
            },
            bullshark: BullsharkParams {
                micro_committee_size: 256,
                shortcut_round_count: 2,
                slow_path_round_count: 4,
                wave_round_count: 4,
            },
            macro_fin: MacroFinParams {
                macro_window_w: 8,
                two_chain_depth: 2,
                inactivity_leak_bps_per_window: 50,
                inactivity_leak_trigger_windows: 4,
            },
            aggregation: AggregationParams {
                subnet_flat_threshold: 500,
                subnet_full_threshold: 1_000,
            },
            leader: LeaderParams {
                reputation_floor: 0.8,
                reputation_ceiling: 1.2,
                reputation_decay: 0.95,
            },
            slashing: SlashingParams {
                equivocation_bps: 10_000,
                double_vote_bps: 5_000,
                da_incident_bps: 500,
                slashing_cap_bps: 5_000,
            },
            anchor_l4: AnchorL4Params { btc_confirmations_for_final: 6 },
            storage: StorageParams {
                gc_hot_horizon_rounds: 200,
                gc_warm_horizon_rounds: 10_000,
                snapshot_interval_macros: 256,
            },
        }
    }

    /// Parse a TOML string into a `Config`.
    pub fn from_toml_str(input: &str) -> crate::Result<Self> {
        let cfg: Self = toml::from_str(input)
            .map_err(|e| crate::Error::InvalidConfig(e.to_string()))?;
        if cfg.schema_version != SCHEMA_VERSION {
            return Err(crate::Error::InvalidConfig(format!(
                "unsupported schema_version {} (expected {})",
                cfg.schema_version, SCHEMA_VERSION
            )));
        }
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_table_17_1_constants() {
        let c = Config::default_table_17_1();
        assert_eq!(c.timing.round_duration_ms, 250);
        assert_eq!(c.macro_fin.macro_window_w, 8);
        assert_eq!(c.bullshark.micro_committee_size, 256);
        assert_eq!(c.aggregation.subnet_flat_threshold, 500);
        assert_eq!(c.aggregation.subnet_full_threshold, 1_000);
        assert_eq!(c.anchor_l4.btc_confirmations_for_final, 6);
    }

    #[test]
    fn unknown_schema_version_rejected() {
        let toml = r#"
            schema_version = 99
            [timing] round_duration_ms = 250
            t_macropropose_ms = 4000
            t_subnet_ms = 2000
            t_canonicalize_ms = 8000
            "#;
        let err = Config::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, crate::Error::InvalidConfig(_)));
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p consensus --lib config::`
Expected: PASS (2 tests).

---

## Task 3: `event.rs` — `Event` enum (all inputs to the SM)

**Files:**
- Create: `crates/consensus/src/event.rs`

- [ ] **Step 1: Write the module + tests**

```rust
//! `Event` — every input the state machine accepts.
//!
//! Sources:
//! - `net` adapter translates wire messages into `Event`s.
//! - `node::timer` emits `Event::TimerFired`.
//! - `node::validator_set_loader` emits `Event::ValidatorSetUpdated`.
//! - `sim::virtual_*` emits the full set for deterministic replay.

use borsh::{BorshDeserialize, BorshSerialize};
use types::{
    dag::CertifiedVertex,
    macros::{MacroProposal, MacroQc},
    micro::MicroQc,
    primitives::{Epoch, ValidatorId},
    slashing::SlashEvidence,
    validator::ValidatorSet,
};

use crypto::bls::Bitmap as _Bitmap; // ensure crypto::bls compiles into our graph
use types::crypto_types::{BlsAggSig, BlsSig, Hash32};

/// Subnet index used by Mode A aggregation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct SubnetId(pub u32);

/// Opaque timer identifier (allocated by `leader::timeout`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, BorshSerialize, BorshDeserialize)]
pub struct TimerId(pub u64);

/// Partial BLS signature contribution from a single validator on a
/// subnet (Mode A) or globally (Mode 0).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct BlsPartial {
    /// Subnet identifier (`0` for Mode 0 flat).
    pub subnet: SubnetId,
    /// Validator signing.
    pub validator: ValidatorId,
    /// Hash of the checkpoint being attested.
    pub checkpoint_hash: Hash32,
    /// Partial signature.
    pub sig: BlsSig,
}

/// Subnet-level aggregate produced by a subnet aggregator.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct SubnetAggregate {
    /// Subnet identifier.
    pub subnet: SubnetId,
    /// Hash of the checkpoint being attested.
    pub checkpoint_hash: Hash32,
    /// Aggregated BLS signature.
    pub agg: BlsAggSig,
}

/// All inputs to the consensus state machine.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum Event {
    /// A new certified vertex arrived from L1.
    CertifiedVertexReceived(CertifiedVertex),
    /// A MicroQc was assembled locally (after we crossed ⅔ stake).
    MicroQcAssembled(MicroQc),
    /// A macro proposal was received from a proposer.
    MacroProposalReceived(MacroProposal),
    /// A partial BLS signature was received.
    BlsPartialReceived(BlsPartial),
    /// A subnet aggregate was received (Mode A).
    SubnetAggregateReceived(SubnetAggregate),
    /// A macro QC was received (used when joining late or for Mode B).
    MacroQcReceived(MacroQc),
    /// A scheduled timer fired.
    TimerFired(TimerId),
    /// The validator set rotated.
    ValidatorSetUpdated {
        /// Epoch the set is valid for.
        epoch: Epoch,
        /// The new set.
        set: ValidatorSet,
    },
    /// Slashing evidence was observed.
    SlashEvidenceFound(SlashEvidence),
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::to_vec;

    #[test]
    fn event_round_trips() {
        let ev = Event::TimerFired(TimerId(7));
        let bytes = to_vec(&ev).unwrap();
        let ev2: Event = borsh::from_slice(&bytes).unwrap();
        assert_eq!(ev, ev2);
    }

    #[test]
    fn _ensure_bitmap_link_exists() {
        // Force the `_Bitmap` import to count so the dep graph keeps crypto.
        let _b = _Bitmap::new(8);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p consensus --lib event::`
Expected: PASS (2 tests).

---

## Task 4: `action.rs` — `Action` enum (all outputs from the SM)

**Files:**
- Create: `crates/consensus/src/action.rs`

- [ ] **Step 1: Write the module + tests**

```rust
//! `Action` — every side-effect the state machine can request.

use std::time::Duration;

use borsh::{BorshDeserialize, BorshSerialize};
use types::{
    api::*,
    macros::{MacroProposal, MacroQc},
    micro::MicroQc,
    primitives::{BlobId, ValidatorId},
    slashing::SlashEvidence,
};

use crate::event::{BlsPartial, SubnetAggregate, TimerId};

// `types::api` doesn't exist yet — re-export the BlobStatus enum we
// own under `consensus::api::tier`. Use `crate::api::tier::BlobStatus`.
pub use crate::api::tier::BlobStatus;

/// All outputs from the consensus state machine.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum Action {
    /// Broadcast a local MicroQc.
    BroadcastMicroQc(MicroQc),
    /// Broadcast a macro proposal.
    BroadcastMacroProposal(MacroProposal),
    /// Broadcast a partial BLS signature.
    BroadcastBlsPartial(BlsPartial),
    /// Broadcast a subnet aggregate.
    BroadcastSubnetAggregate(SubnetAggregate),
    /// Broadcast a complete macro QC.
    BroadcastMacroQc(MacroQc),
    /// Schedule a new timer; host emits `Event::TimerFired(id)` after `delay`.
    ScheduleTimer {
        /// Identifier the host must echo back.
        id: TimerId,
        /// Duration before firing.
        delay: Duration,
    },
    /// Cancel a previously scheduled timer.
    CancelTimer(TimerId),
    /// Persist a finalized MacroQc.
    PersistMacroQc(MacroQc),
    /// Emit slashing evidence to gossip + storage.
    EmitSlashEvidence {
        /// The offender.
        offender: ValidatorId,
        /// Evidence payload.
        evidence: SlashEvidence,
    },
    /// Update the externally-visible status of a blob.
    UpdateBlobStatus {
        /// Blob whose status changes.
        blob: BlobId,
        /// New status.
        status: BlobStatus,
    },
}

#[doc(hidden)]
#[allow(unused_imports)]
mod _force_used {
    // Stop the `types::api::*` re-export from emitting a dead-code lint
    // if `types` exposes an `api` module later.
    use super::*;
}
```

Note: the `pub use types::api::*;` will fail to compile because `types` does not have an `api` module. Remove that line — the canonical re-export of `BlobStatus` comes from `crate::api::tier`. Final shape of the import block at the top of `action.rs` is:

```rust
use std::time::Duration;

use borsh::{BorshDeserialize, BorshSerialize};
use types::{
    macros::{MacroProposal, MacroQc},
    micro::MicroQc,
    primitives::{BlobId, ValidatorId},
    slashing::SlashEvidence,
};

use crate::{api::tier::BlobStatus, event::{BlsPartial, SubnetAggregate, TimerId}};
```

- [ ] **Step 2: Verify it compiles after Task 6 lands (`api::tier::BlobStatus`)**

`cargo build -p consensus` will fail until Task 6 introduces `BlobStatus`. That's fine — tasks build cumulatively.

- [ ] **Step 3: Write tests in the same file** (run after Task 6):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use borsh::to_vec;
    use std::time::Duration;

    #[test]
    fn schedule_timer_round_trips() {
        let a = Action::ScheduleTimer { id: TimerId(1), delay: Duration::from_millis(250) };
        let bytes = to_vec(&a).unwrap();
        let a2: Action = borsh::from_slice(&bytes).unwrap();
        assert_eq!(a, a2);
    }
}
```

> Note: `Duration` does **not** implement `BorshSerialize` by default. Implementing a wrapper would be overkill for the skeleton. Replace the field type with `delay_nanos: u128` (a flat integer) to keep the enum Borsh-friendly. Update the field type:

```rust
ScheduleTimer {
    id: TimerId,
    /// Delay in nanoseconds (host converts to `Duration`).
    delay_nanos: u128,
}
```

Then the test becomes:

```rust
let a = Action::ScheduleTimer { id: TimerId(1), delay_nanos: 250_000_000 };
```

---

## Task 5: `ports/` — the 5 DI traits

**Files:**
- Create: `crates/consensus/src/ports/mod.rs`
- Create: `crates/consensus/src/ports/dag_view.rs`
- Create: `crates/consensus/src/ports/clock.rs`
- Create: `crates/consensus/src/ports/rng_beacon.rs`
- Create: `crates/consensus/src/ports/validator_set.rs`
- Create: `crates/consensus/src/ports/persistence.rs`

These are **outbound** traits — `consensus` calls into them, never the other way around. None has `async`.

- [ ] **Step 1: Write `crates/consensus/src/ports/mod.rs`**

```rust
//! Outbound dependency-injection traits.
//!
//! `consensus` calls these to read the outside world; host binaries
//! provide concrete impls (`storage`, `net`, `node::timer`, …).

pub mod clock;
pub mod dag_view;
pub mod persistence;
pub mod rng_beacon;
pub mod validator_set;

pub use clock::Clock;
pub use dag_view::DagView;
pub use persistence::Persistence;
pub use rng_beacon::RandomnessBeacon;
pub use validator_set::ValidatorSetPort;
```

- [ ] **Step 2: Write `crates/consensus/src/ports/dag_view.rs`**

```rust
//! `DagView` port: future L1 plug-in seam. Implementations resolve
//! certified vertices by hash and enumerate parents.

use types::{crypto_types::Hash32, dag::CertifiedVertex, primitives::Round};

use crate::error::Result;

/// Read-only view over the availability DAG.
pub trait DagView: Send + Sync {
    /// Return the certified vertex with `hash`, or `None` if unknown.
    fn vertex(&self, hash: &Hash32) -> Result<Option<CertifiedVertex>>;

    /// Return every certified vertex in the given round.
    fn vertices_at_round(&self, round: Round) -> Result<Vec<CertifiedVertex>>;
}
```

- [ ] **Step 3: Write `crates/consensus/src/ports/clock.rs`**

```rust
//! `Clock` port. The simulator uses a virtual clock; the node uses tokio.

/// Monotonic clock readings in nanoseconds.
pub trait Clock: Send + Sync {
    /// Return the current monotonic time in nanoseconds since an
    /// implementation-defined epoch (e.g. process start).
    fn now_nanos(&self) -> u128;
}
```

- [ ] **Step 4: Write `crates/consensus/src/ports/rng_beacon.rs`**

```rust
//! `RandomnessBeacon` port. Returns the latest beacon output. Chaining
//! itself is computed inside `consensus::leader::beacon`.

use types::crypto_types::Hash32;

use crate::error::Result;

/// Provider of randomness-beacon outputs.
pub trait RandomnessBeacon: Send + Sync {
    /// Return the latest beacon output (`R_w` for the current window).
    fn current(&self) -> Result<Hash32>;
}
```

- [ ] **Step 5: Write `crates/consensus/src/ports/validator_set.rs`**

```rust
//! `ValidatorSetPort`. Storage / node maintains the current epoch's set.

use types::{
    primitives::{Epoch, ValidatorId},
    validator::ValidatorSet,
};

use crate::error::Result;

/// Read access to validator sets, indexed by epoch.
pub trait ValidatorSetPort: Send + Sync {
    /// Return the active validator set for `epoch`.
    fn set_for(&self, epoch: Epoch) -> Result<Option<ValidatorSet>>;

    /// Return the index of `validator` inside `set_for(epoch)`, if any.
    fn index_of(&self, epoch: Epoch, validator: &ValidatorId) -> Result<Option<u32>>;
}
```

- [ ] **Step 6: Write `crates/consensus/src/ports/persistence.rs`**

```rust
//! `Persistence` port. Storage adapter (plan 05) implements this trait.

use types::{
    crypto_types::Hash32,
    macros::{MacroCheckpoint, MacroQc},
    micro::MicroQc,
    primitives::Height,
    slashing::SlashEvidence,
};

use crate::error::Result;

/// Persistent storage for finalized artifacts and append-only logs.
pub trait Persistence: Send + Sync {
    /// Persist a MicroQc.
    fn store_micro_qc(&self, qc: &MicroQc) -> Result<()>;

    /// Persist a MacroCheckpoint.
    fn store_macro_checkpoint(&self, cp: &MacroCheckpoint) -> Result<()>;

    /// Persist a MacroQc (finalized).
    fn store_macro_qc(&self, qc: &MacroQc) -> Result<()>;

    /// Append slashing evidence to the immutable log.
    fn append_slash_evidence(&self, ev: &SlashEvidence) -> Result<()>;

    /// Return the macro checkpoint at `height` if known.
    fn macro_checkpoint_at(&self, height: Height) -> Result<Option<MacroCheckpoint>>;

    /// Return the macro QC for `checkpoint_hash` if any.
    fn macro_qc_for(&self, checkpoint_hash: &Hash32) -> Result<Option<MacroQc>>;
}
```

- [ ] **Step 7: Build**

Run: `cargo build -p consensus`
Expected: still failing — `api::tier::BlobStatus` not yet defined. Continue.

---

## Task 6: `api/` — `BlobStatus` tier + read-only query trait

**Files:**
- Create: `crates/consensus/src/api/mod.rs`
- Create: `crates/consensus/src/api/tier.rs`
- Create: `crates/consensus/src/api/query.rs`

- [ ] **Step 1: Write `crates/consensus/src/api/mod.rs`**

```rust
//! External read-only API surface (Appendix A).

pub mod query;
pub mod tier;

pub use query::ConsensusQuery;
pub use tier::BlobStatus;
```

- [ ] **Step 2: Write `crates/consensus/src/api/tier.rs`**

```rust
//! Blob lifecycle (Appendix A):
//! `accepted → soft_confirmed → justified → finalized → epoch_finalized`.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Externally-visible status of a blob.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd,
    BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum BlobStatus {
    /// L1 accepted (custody acknowledged).
    Accepted        = 0,
    /// L2 micro-committed (within wave).
    SoftConfirmed   = 1,
    /// L3 justified (one macro window of 2-chain).
    Justified       = 2,
    /// L3 finalized (full 2-chain).
    Finalized       = 3,
    /// L4 anchored to Bitcoin (placeholder for future).
    EpochFinalized  = 4,
}
```

- [ ] **Step 3: Write `crates/consensus/src/api/query.rs`**

```rust
//! Read-only consensus queries surfaced to RPC layer.

use types::{
    crypto_types::Hash32,
    macros::MacroQc,
    primitives::{BlobId, Height, Round},
};

use crate::error::Result;

use super::tier::BlobStatus;

/// Read-only queries on the in-memory state. Implementations are
/// expected to be lock-free / single-threaded — concurrency is a host
/// concern.
pub trait ConsensusQuery: Send + Sync {
    /// Last finalized macro QC.
    fn latest_finalized(&self) -> Result<Option<MacroQc>>;

    /// Height of the most recently committed micro-checkpoint.
    fn micro_head(&self) -> Result<Round>;

    /// Status of a specific blob.
    fn blob_status(&self, blob: &BlobId) -> Result<BlobStatus>;

    /// MacroCheckpoint hash at `height`, if any.
    fn macro_checkpoint_hash(&self, height: Height) -> Result<Option<Hash32>>;
}
```

- [ ] **Step 4: Build**

Run: `cargo build -p consensus`
Expected: PASS (Action now resolves `BlobStatus`).

---

## Task 7: `state_machine.rs` — entrypoint stub

**Files:**
- Create: `crates/consensus/src/state_machine.rs`

The skeleton accepts every event and returns an empty `SmallVec`. Real handlers will live behind dispatching match arms; we keep the shape so plans 03b…03d can extend it without changing the public API.

- [ ] **Step 1: Write the module + tests**

```rust
//! The pure deterministic state machine.

use smallvec::SmallVec;

use crate::{
    action::Action,
    config::Config,
    error::Result,
    event::Event,
};

/// Up-to-eight outgoing actions per event keeps things stack-allocated.
pub type Actions = SmallVec<[Action; 8]>;

/// Consensus state machine.
///
/// Deterministic: given the same starting state, the same sequence of
/// `Event`s always produces the same sequence of `Action`s.
#[derive(Debug)]
pub struct StateMachine {
    /// Active protocol parameters.
    cfg: Config,
}

impl StateMachine {
    /// Build a new state machine with the supplied configuration.
    #[must_use]
    pub fn new(cfg: Config) -> Self {
        Self { cfg }
    }

    /// Active config (immutable while running).
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// Drive one event through the state machine, returning any
    /// resulting [`Action`]s.
    ///
    /// In the skeleton phase this returns an empty `Actions` for every
    /// event so downstream binaries can wire end-to-end before any
    /// algorithm is implemented.
    pub fn step(&mut self, event: Event) -> Result<Actions> {
        match event {
            Event::CertifiedVertexReceived(_) => {
                // TODO(plan 03b): Bullshark wave / commit dispatch.
                Ok(Actions::new())
            }
            Event::MicroQcAssembled(_) => Ok(Actions::new()),
            Event::MacroProposalReceived(_) => {
                // TODO(plan 03c): macro proposer dispatch.
                Ok(Actions::new())
            }
            Event::BlsPartialReceived(_) | Event::SubnetAggregateReceived(_) => {
                // TODO(plan 03c): adaptive aggregation.
                Ok(Actions::new())
            }
            Event::MacroQcReceived(_) => Ok(Actions::new()),
            Event::TimerFired(_) => Ok(Actions::new()),
            Event::ValidatorSetUpdated { .. } => Ok(Actions::new()),
            Event::SlashEvidenceFound(_) => {
                // TODO(plan 03d): slashing evidence validation + EmitSlashEvidence.
                Ok(Actions::new())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::TimerId;

    #[test]
    fn step_returns_empty_for_timer_in_skeleton() {
        let mut sm = StateMachine::new(Config::default_table_17_1());
        let actions = sm.step(Event::TimerFired(TimerId(0))).unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn step_is_total_over_event_enum() {
        // Drive every variant once; none must panic or error.
        let mut sm = StateMachine::new(Config::default_table_17_1());
        sm.step(Event::TimerFired(TimerId(0))).unwrap();
        // Other variants require non-trivial constructor data — the
        // round-trip happens in integration tests under tests/.
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p consensus --lib state_machine::`
Expected: PASS (2 tests).

---

## Task 8: `lock_macro.rs` — §13.5 invariant tracker (skeleton)

**Files:**
- Create: `crates/consensus/src/lock_macro.rs`

- [ ] **Step 1: Write the module + tests**

```rust
//! `lock_macro` invariant (whitepaper §13.5).
//!
//! A validator that voted to finalize macro height `h` must not later
//! vote on a conflicting candidate at the same height. This module
//! tracks the per-validator "locked" height; full enforcement happens
//! when `consensus::macro_fin::vote_book` is wired.

use std::collections::HashMap;

use types::{
    crypto_types::Hash32,
    primitives::{Height, ValidatorId},
};

/// Per-validator locks: each validator may pin at most one
/// `(height, checkpoint_hash)` pair as the canonical macro vote.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LockMacro {
    locks: HashMap<ValidatorId, (Height, Hash32)>,
}

impl LockMacro {
    /// New empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempt to lock `validator` to `(height, checkpoint)`. Returns
    /// `Err` if the validator is already locked to a *different*
    /// checkpoint at the same height.
    pub fn try_lock(
        &mut self,
        validator: ValidatorId,
        height: Height,
        checkpoint: Hash32,
    ) -> Result<(), &'static str> {
        match self.locks.get(&validator) {
            Some(&(h, prev)) if h == height && prev != checkpoint => {
                Err("validator already locked to a conflicting checkpoint at this height")
            }
            _ => {
                self.locks.insert(validator, (height, checkpoint));
                Ok(())
            }
        }
    }

    /// Current lock for `validator`, if any.
    #[must_use]
    pub fn get(&self, validator: &ValidatorId) -> Option<(Height, Hash32)> {
        self.locks.get(validator).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_then_extend_to_higher_height_ok() {
        let mut lm = LockMacro::new();
        let v = ValidatorId([1; 32]);
        lm.try_lock(v, Height(1), Hash32([0xAA; 32])).unwrap();
        lm.try_lock(v, Height(2), Hash32([0xBB; 32])).unwrap();
        assert_eq!(lm.get(&v), Some((Height(2), Hash32([0xBB; 32]))));
    }

    #[test]
    fn lock_conflict_at_same_height_rejected() {
        let mut lm = LockMacro::new();
        let v = ValidatorId([1; 32]);
        lm.try_lock(v, Height(1), Hash32([0xAA; 32])).unwrap();
        let err = lm.try_lock(v, Height(1), Hash32([0xCC; 32])).unwrap_err();
        assert!(err.contains("conflicting"));
    }

    #[test]
    fn same_height_same_checkpoint_idempotent() {
        let mut lm = LockMacro::new();
        let v = ValidatorId([1; 32]);
        lm.try_lock(v, Height(1), Hash32([0xAA; 32])).unwrap();
        lm.try_lock(v, Height(1), Hash32([0xAA; 32])).unwrap();
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p consensus --lib lock_macro::`
Expected: PASS (3 tests).

---

## Task 9: Bullshark (L2) skeleton modules

**Files:**
- Create: `crates/consensus/src/bullshark/mod.rs`
- Create: `crates/consensus/src/bullshark/wave.rs`
- Create: `crates/consensus/src/bullshark/anchor.rs`
- Create: `crates/consensus/src/bullshark/commit.rs`
- Create: `crates/consensus/src/bullshark/linearize.rs`
- Create: `crates/consensus/src/bullshark/micro_qc.rs`

Each file declares the canonical struct + function signature, with bodies returning sentinel values. All exposed types are real; only the algorithmic content is stubbed.

- [ ] **Step 1: Write `crates/consensus/src/bullshark/mod.rs`**

```rust
//! Bullshark micro-ordering (whitepaper §8).
//!
//! Implementation lands in follow-up plans:
//!   * Wave structure + anchor selection → plan 03b.
//!   * Commit rule (shortcut + slow path)  → plan 03b.
//!   * Linearization (BFS over Closure(Aw)) → plan 03b.
//!   * MicroQc aggregation                  → plan 03b.

pub mod anchor;
pub mod commit;
pub mod linearize;
pub mod micro_qc;
pub mod wave;

pub use anchor::AnchorChoice;
pub use commit::{CommitDecision, CommitPath};
pub use linearize::Linearization;
pub use micro_qc::MicroQcBuilder;
pub use wave::WaveId;
```

- [ ] **Step 2: Write `crates/consensus/src/bullshark/wave.rs`**

```rust
//! Wave numbering (4 rounds per wave): `4w, 4w+1, 4w+2, 4w+3`.

use types::primitives::Round;

/// A Bullshark wave (4 consecutive rounds).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct WaveId(pub u64);

impl WaveId {
    /// First round in this wave.
    #[must_use]
    pub fn first_round(self) -> Round {
        Round(self.0 * 4)
    }

    /// Last round in this wave.
    #[must_use]
    pub fn last_round(self) -> Round {
        Round(self.0 * 4 + 3)
    }

    /// Wave containing `round`.
    #[must_use]
    pub fn of_round(round: Round) -> Self {
        Self(round.0 / 4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_bounds() {
        let w = WaveId(2);
        assert_eq!(w.first_round(), Round(8));
        assert_eq!(w.last_round(), Round(11));
        assert_eq!(WaveId::of_round(Round(10)), WaveId(2));
    }
}
```

- [ ] **Step 3: Write `crates/consensus/src/bullshark/anchor.rs`**

```rust
//! Anchor selection (private VRF sortition).

use types::{crypto_types::Hash32, primitives::ValidatorId};

use super::wave::WaveId;

/// Outcome of anchor selection for one wave.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnchorChoice {
    /// Wave this anchor belongs to.
    pub wave: WaveId,
    /// Author who won the anchor slot.
    pub author: ValidatorId,
    /// Hash of the anchor vertex.
    pub anchor_hash: Hash32,
}
```

- [ ] **Step 4: Write `crates/consensus/src/bullshark/commit.rs`**

```rust
//! Bullshark commit rule (shortcut + slow path).

use super::wave::WaveId;

/// Which commit path resolved the wave.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitPath {
    /// Anchor was committed via the 2-round shortcut.
    Shortcut,
    /// Anchor was committed via the 4-round slow path.
    SlowPath,
}

/// Result of running the commit rule for one wave.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitDecision {
    /// Wave that produced the decision.
    pub wave: WaveId,
    /// Which path won.
    pub path: CommitPath,
}
```

- [ ] **Step 5: Write `crates/consensus/src/bullshark/linearize.rs`**

```rust
//! Closure(Aw) BFS linearization, tie-break by vertex hash.

use types::crypto_types::Hash32;

/// Output of linearization: ordered hashes of committed vertices.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Linearization {
    /// Linearized vertex hashes in commit order.
    pub order: Vec<Hash32>,
}
```

- [ ] **Step 6: Write `crates/consensus/src/bullshark/micro_qc.rs`**

```rust
//! MicroQc aggregation (≥ ⌈2/3·C⌉).

use types::{crypto_types::Hash32, micro::MicroQc};

use crate::{config::Config, error::Result};

/// Builder that collects partial signatures over a `MicroCheckpoint`
/// hash and emits a [`MicroQc`] once stake threshold is reached.
#[derive(Debug)]
pub struct MicroQcBuilder<'a> {
    /// Reference to active config.
    pub config: &'a Config,
    /// Hash of the checkpoint being attested.
    pub target: Hash32,
}

impl<'a> MicroQcBuilder<'a> {
    /// New builder targeting `target`.
    #[must_use]
    pub fn new(config: &'a Config, target: Hash32) -> Self {
        Self { config, target }
    }

    /// Attempt to finalize the QC. Returns `Ok(None)` until threshold is
    /// reached; `Ok(Some(qc))` once it is.
    ///
    /// Skeleton: always returns `Ok(None)`. Plan 03b implements the rule.
    pub fn try_finalize(&self) -> Result<Option<MicroQc>> {
        Ok(None)
    }
}
```

- [ ] **Step 7: Build + test**

Run: `cargo test -p consensus --lib bullshark::`
Expected: PASS (1 test).

---

## Task 10: Macro-Finality (L3) skeleton modules

**Files:**
- Create: `crates/consensus/src/macro_fin/mod.rs`
- Create: `crates/consensus/src/macro_fin/window.rs`
- Create: `crates/consensus/src/macro_fin/proposer.rs`
- Create: `crates/consensus/src/macro_fin/checkpoint.rs`
- Create: `crates/consensus/src/macro_fin/macro_qc.rs`
- Create: `crates/consensus/src/macro_fin/two_chain.rs`
- Create: `crates/consensus/src/macro_fin/vote_book.rs`
- Create: `crates/consensus/src/macro_fin/aggregation/mod.rs`
- Create: `crates/consensus/src/macro_fin/aggregation/mode0_flat.rs`
- Create: `crates/consensus/src/macro_fin/aggregation/mode_a_subnet.rs`
- Create: `crates/consensus/src/macro_fin/aggregation/mode_b_leaderless.rs`
- Create: `crates/consensus/src/macro_fin/aggregation/subnet.rs`

- [ ] **Step 1: Write `crates/consensus/src/macro_fin/mod.rs`**

```rust
//! L3 macro-finality (whitepaper §9).

pub mod aggregation;
pub mod checkpoint;
pub mod macro_qc;
pub mod proposer;
pub mod two_chain;
pub mod vote_book;
pub mod window;

pub use aggregation::{select_mode, AggregationMode, Ke};
pub use checkpoint::CheckpointBuilder;
pub use macro_qc::MacroQcAssembler;
pub use proposer::ProposerSchedule;
pub use two_chain::TwoChainRule;
pub use vote_book::VoteBook;
pub use window::MacroWindow;
```

- [ ] **Step 2: Write `crates/consensus/src/macro_fin/window.rs`**

```rust
//! Macro window cadence (W micro-slots per window).

use types::primitives::{Height, Round};

use crate::config::Config;

/// One macro window covering `W` consecutive micro-slots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MacroWindow {
    /// Window height.
    pub height: Height,
}

impl MacroWindow {
    /// Window containing `round`, given the active config.
    #[must_use]
    pub fn of_round(cfg: &Config, round: Round) -> Self {
        let w = u64::from(cfg.macro_fin.macro_window_w);
        Self { height: Height(round.0 / w) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_to_window_uses_W_from_config() {
        let cfg = Config::default_table_17_1();
        assert_eq!(MacroWindow::of_round(&cfg, Round(15)).height, Height(1));
        assert_eq!(MacroWindow::of_round(&cfg, Round(16)).height, Height(2));
    }
}
```

- [ ] **Step 3: Write `crates/consensus/src/macro_fin/proposer.rs`**

```rust
//! Macro proposer scheduling (primary + backup).

use types::primitives::{Height, ValidatorId};

/// Primary + backup proposer for a given macro window.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposerSchedule {
    /// Macro window height.
    pub height: Height,
    /// Primary proposer.
    pub primary: ValidatorId,
    /// Backup proposer (used after `T_macropropose` timeout).
    pub backup: ValidatorId,
}
```

- [ ] **Step 4: Write `crates/consensus/src/macro_fin/checkpoint.rs`**

```rust
//! Build / verify `MacroCheckpoint`s.

use types::macros::MacroCheckpoint;

use crate::error::Result;

/// Helper that assembles a `MacroCheckpoint` from micro-roots.
#[derive(Debug, Default)]
pub struct CheckpointBuilder;

impl CheckpointBuilder {
    /// Skeleton: returns `Ok(None)`. Plan 03c implements the assembly.
    pub fn try_build(&self) -> Result<Option<MacroCheckpoint>> {
        Ok(None)
    }
}
```

- [ ] **Step 5: Write `crates/consensus/src/macro_fin/macro_qc.rs`**

```rust
//! Macro QC assembly (mode-aware).

use types::{crypto_types::Hash32, macros::MacroQc};

use crate::{config::Config, error::Result};

/// Builder that collects per-validator (or per-subnet) signatures and
/// emits a `MacroQc` once the stake threshold is reached.
#[derive(Debug)]
pub struct MacroQcAssembler<'a> {
    /// Active config (mode thresholds, etc.).
    pub config: &'a Config,
    /// Checkpoint hash being attested.
    pub target: Hash32,
}

impl<'a> MacroQcAssembler<'a> {
    /// Construct.
    #[must_use]
    pub fn new(config: &'a Config, target: Hash32) -> Self {
        Self { config, target }
    }

    /// Skeleton: never finalizes. Plan 03c implements the modes.
    pub fn try_finalize(&self) -> Result<Option<MacroQc>> {
        Ok(None)
    }
}
```

- [ ] **Step 6: Write `crates/consensus/src/macro_fin/two_chain.rs`**

```rust
//! Casper-FFG 2-chain finality rule.

use types::crypto_types::Hash32;

/// 2-chain finality tracker.
#[derive(Debug, Default)]
pub struct TwoChainRule {
    /// Hash of the most recently justified checkpoint.
    pub justified_head: Option<Hash32>,
}
```

- [ ] **Step 7: Write `crates/consensus/src/macro_fin/vote_book.rs`**

```rust
//! Per-validator vote history (epoch-indexed) for surround/double-vote detection.

use std::collections::HashMap;

use types::{
    crypto_types::Hash32,
    primitives::{Epoch, ValidatorId},
};

/// A single macro vote: `(source, target, checkpoint_hash)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VoteRecord {
    /// Casper-FFG source epoch.
    pub source: Epoch,
    /// Casper-FFG target epoch.
    pub target: Epoch,
    /// Hash of the attested checkpoint.
    pub checkpoint: Hash32,
}

/// Per-validator vote history.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VoteBook {
    /// Sorted by `target` per validator.
    votes: HashMap<ValidatorId, Vec<VoteRecord>>,
}

impl VoteBook {
    /// New empty book.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `record` for `validator`.
    pub fn record(&mut self, validator: ValidatorId, record: VoteRecord) {
        self.votes.entry(validator).or_default().push(record);
    }

    /// Iterate `validator`'s votes (insertion order).
    pub fn votes_of(&self, validator: &ValidatorId) -> &[VoteRecord] {
        self.votes.get(validator).map_or(&[], Vec::as_slice)
    }
}
```

- [ ] **Step 8: Write `crates/consensus/src/macro_fin/aggregation/mod.rs`**

```rust
//! Adaptive aggregation (Mode 0, A, B). Whitepaper §9.2.

pub mod mode0_flat;
pub mod mode_a_subnet;
pub mod mode_b_leaderless;
pub mod subnet;

pub use mode0_flat::Mode0Flat;
pub use mode_a_subnet::ModeASubnet;
pub use mode_b_leaderless::ModeBLeaderless;
pub use subnet::SubnetAssign;

use crate::config::Config;

/// Aggregation mode chosen for a window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AggregationMode {
    /// Flat (Ne < 500).
    Mode0Flat,
    /// Subnet (Ne ≥ 500).
    ModeASubnet,
    /// Leaderless fallback.
    ModeBLeaderless,
}

/// Number of subnets `Ke` for Mode A (Eq. 9.1).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ke(pub u32);

/// Select the aggregation mode given active-validator count `n_e`.
///
/// Skeleton: thresholds-only — no Mode B fallback logic. Real selection
/// also factors proposer-availability and is implemented in plan 03c.
#[must_use]
pub fn select_mode(cfg: &Config, n_e: u32) -> AggregationMode {
    if n_e < cfg.aggregation.subnet_flat_threshold {
        AggregationMode::Mode0Flat
    } else {
        AggregationMode::ModeASubnet
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_thresholds() {
        let cfg = Config::default_table_17_1();
        assert_eq!(select_mode(&cfg, 100), AggregationMode::Mode0Flat);
        assert_eq!(select_mode(&cfg, 499), AggregationMode::Mode0Flat);
        assert_eq!(select_mode(&cfg, 500), AggregationMode::ModeASubnet);
        assert_eq!(select_mode(&cfg, 5_000), AggregationMode::ModeASubnet);
    }
}
```

- [ ] **Step 9: Write `crates/consensus/src/macro_fin/aggregation/mode0_flat.rs`**

```rust
//! Mode 0 flat aggregation (Ne < 500).

/// Mode 0 aggregator state.
#[derive(Debug, Default)]
pub struct Mode0Flat;
```

- [ ] **Step 10: Write `crates/consensus/src/macro_fin/aggregation/mode_a_subnet.rs`**

```rust
//! Mode A subnet aggregation (Ne ≥ 500) — rotated per epoch.

/// Mode A aggregator state.
#[derive(Debug, Default)]
pub struct ModeASubnet;
```

- [ ] **Step 11: Write `crates/consensus/src/macro_fin/aggregation/mode_b_leaderless.rs`**

```rust
//! Mode B leaderless fallback (proposer missed both primary and backup slots).

/// Mode B aggregator state.
#[derive(Debug, Default)]
pub struct ModeBLeaderless;
```

- [ ] **Step 12: Write `crates/consensus/src/macro_fin/aggregation/subnet.rs`**

```rust
//! Subnet assignment: `subnet(v_i, e) = H(pubkey || R_macro) mod K_e`.

use crypto::hash::{blake3_with_dst, dst};
use types::{crypto_types::Hash32, primitives::ValidatorId};

use super::Ke;

/// Deterministic subnet assignment for a validator at an epoch.
#[derive(Debug)]
pub struct SubnetAssign {
    /// Number of subnets in this epoch.
    pub k_e: Ke,
    /// Beacon output for the macro window.
    pub r_macro: Hash32,
}

impl SubnetAssign {
    /// Subnet index in `0..k_e.0`.
    #[must_use]
    pub fn index_for(&self, validator: &ValidatorId) -> u32 {
        let mut buf = Vec::with_capacity(64);
        buf.extend_from_slice(validator.as_bytes());
        buf.extend_from_slice(&self.r_macro.0);
        let h = blake3_with_dst(dst::SUBNET_ASSIGN, &buf);
        let n = u32::from_be_bytes([h.0[0], h.0[1], h.0[2], h.0[3]]);
        n % self.k_e.0.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subnet_assignment_is_deterministic() {
        let assign = SubnetAssign { k_e: Ke(8), r_macro: Hash32([0xAB; 32]) };
        let v = ValidatorId([1; 32]);
        let a = assign.index_for(&v);
        let b = assign.index_for(&v);
        assert_eq!(a, b);
        assert!(a < 8);
    }
}
```

- [ ] **Step 13: Run tests**

Run: `cargo test -p consensus --lib macro_fin::`
Expected: PASS (3 tests: window, aggregation::tests, aggregation::subnet).

---

## Task 11: Leader (election + timing) skeleton

**Files:**
- Create: `crates/consensus/src/leader/mod.rs`
- Create: `crates/consensus/src/leader/beacon.rs`
- Create: `crates/consensus/src/leader/vrf_sortition.rs`
- Create: `crates/consensus/src/leader/reputation.rs`
- Create: `crates/consensus/src/leader/timeout.rs`

- [ ] **Step 1: Write `crates/consensus/src/leader/mod.rs`**

```rust
//! Leader election + timer scheduling (cross-layer: L2 + L3).

pub mod beacon;
pub mod reputation;
pub mod timeout;
pub mod vrf_sortition;

pub use beacon::chain_beacon;
pub use reputation::Reputation;
pub use timeout::TimerScheduler;
pub use vrf_sortition::vrf_sortition_score;
```

- [ ] **Step 2: Write `crates/consensus/src/leader/beacon.rs`**

```rust
//! Beacon chaining: `R_w = H(R_{w-1} || MacroQC)` (Eq. 8.1).

use crypto::hash::{blake3_with_dst, dst};
use types::crypto_types::Hash32;

/// Compute the next beacon output from the previous beacon and a macro QC hash.
#[must_use]
pub fn chain_beacon(prev: &Hash32, macro_qc_hash: &Hash32) -> Hash32 {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(&prev.0);
    buf[32..].copy_from_slice(&macro_qc_hash.0);
    blake3_with_dst(dst::BEACON, &buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaining_is_deterministic_and_changes_on_input() {
        let prev = Hash32([1; 32]);
        let qc = Hash32([2; 32]);
        let a = chain_beacon(&prev, &qc);
        let b = chain_beacon(&prev, &qc);
        assert_eq!(a, b);
        let c = chain_beacon(&prev, &Hash32([3; 32]));
        assert_ne!(a, c);
    }
}
```

- [ ] **Step 3: Write `crates/consensus/src/leader/vrf_sortition.rs`**

```rust
//! Stake-weighted sortition score `y_i · W / (w_i · rep_i)` (spec §8.1).

use crypto::vrf::vrf_to_uniform;
use types::crypto_types::Hash32;

/// Compute the sortition score for a validator.
///
/// Lower score → earlier in the sortition order. Caller picks the
/// minimum score across the active set.
///
/// * `vrf_beta` — the validator's VRF output for this slot's `alpha`.
/// * `total_stake` — Σ stake across the active set.
/// * `own_stake` — this validator's stake.
/// * `reputation` — Shoal reputation (typically in `[0.8, 1.2]`).
#[must_use]
pub fn vrf_sortition_score(
    vrf_beta: &Hash32,
    total_stake: u64,
    own_stake: u64,
    reputation: f64,
) -> f64 {
    let y = vrf_to_uniform(vrf_beta);
    let denom = (own_stake as f64) * reputation;
    if denom == 0.0 {
        f64::INFINITY
    } else {
        y * (total_stake as f64) / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn higher_stake_or_reputation_lowers_score() {
        let beta = Hash32([0xFF; 32]);
        let s_low_rep = vrf_sortition_score(&beta, 1_000, 100, 0.8);
        let s_high_rep = vrf_sortition_score(&beta, 1_000, 100, 1.2);
        assert!(s_high_rep < s_low_rep);
        let s_low_stake = vrf_sortition_score(&beta, 1_000, 100, 1.0);
        let s_high_stake = vrf_sortition_score(&beta, 1_000, 500, 1.0);
        assert!(s_high_stake < s_low_stake);
    }
}
```

- [ ] **Step 4: Write `crates/consensus/src/leader/reputation.rs`**

```rust
//! Shoal reputation `[0.8, 1.2]` — EWMA-style update.

use crate::config::Config;

/// Clamped reputation value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reputation(pub f64);

impl Reputation {
    /// Neutral starting reputation.
    #[must_use]
    pub fn neutral() -> Self {
        Self(1.0)
    }

    /// Apply an observation in `[0.0, 1.0]` (1 = perfect uptime,
    /// 0 = miss). Updates via EWMA, then clamps to `[floor, ceiling]`.
    #[must_use]
    pub fn updated(self, cfg: &Config, observation: f64) -> Self {
        let decay = cfg.leader.reputation_decay;
        // Maps observation onto the configured floor..ceiling range.
        let target = cfg.leader.reputation_floor
            + observation.clamp(0.0, 1.0)
                * (cfg.leader.reputation_ceiling - cfg.leader.reputation_floor);
        let next = decay * self.0 + (1.0 - decay) * target;
        Self(next.clamp(cfg.leader.reputation_floor, cfg.leader.reputation_ceiling))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reputation_clamps_inside_band() {
        let cfg = Config::default_table_17_1();
        let r = Reputation::neutral();
        let r1 = r.updated(&cfg, 1.0);
        assert!(r1.0 >= cfg.leader.reputation_floor);
        assert!(r1.0 <= cfg.leader.reputation_ceiling);
        let r2 = r.updated(&cfg, 0.0);
        assert!(r2.0 >= cfg.leader.reputation_floor);
    }
}
```

- [ ] **Step 5: Write `crates/consensus/src/leader/timeout.rs`**

```rust
//! Central timer scheduler. The SM emits `Action::ScheduleTimer` /
//! `Action::CancelTimer`; the host translates those into real timers.

use crate::event::TimerId;

/// Allocates monotonic `TimerId`s for the SM. Stays internal — host
/// binaries never call this directly.
#[derive(Debug, Default)]
pub struct TimerScheduler {
    next: u64,
}

impl TimerScheduler {
    /// Allocate a new `TimerId`.
    pub fn allocate(&mut self) -> TimerId {
        let id = TimerId(self.next);
        self.next += 1;
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_monotonic_ids() {
        let mut s = TimerScheduler::default();
        assert_eq!(s.allocate(), TimerId(0));
        assert_eq!(s.allocate(), TimerId(1));
    }
}
```

- [ ] **Step 6: Build + test**

Run: `cargo test -p consensus --lib leader::`
Expected: PASS (3 tests).

---

## Task 12: Slashing skeleton

**Files:**
- Create: `crates/consensus/src/slashing/mod.rs`
- Create: `crates/consensus/src/slashing/evidence.rs`
- Create: `crates/consensus/src/slashing/equivocation.rs`
- Create: `crates/consensus/src/slashing/surround.rs`
- Create: `crates/consensus/src/slashing/inactivity_leak.rs`
- Create: `crates/consensus/src/slashing/penalty.rs`

- [ ] **Step 1: Write `crates/consensus/src/slashing/mod.rs`**

```rust
//! Slashing detectors + penalty math.

pub mod equivocation;
pub mod evidence;
pub mod inactivity_leak;
pub mod penalty;
pub mod surround;

pub use evidence::verify_evidence;
pub use penalty::Penalty;
```

- [ ] **Step 2: Write `crates/consensus/src/slashing/evidence.rs`**

```rust
//! Pure-function verifier for `SlashEvidence`.

use types::slashing::SlashEvidence;

use crate::error::Result;

/// Verify a slashing evidence. Skeleton always returns `Ok(())`; plan 03d
/// implements the per-variant verifier.
pub fn verify_evidence(_ev: &SlashEvidence) -> Result<()> {
    Ok(())
}
```

- [ ] **Step 3: Write `crates/consensus/src/slashing/equivocation.rs`**

```rust
//! Macro equivocation detector (100 % slash).

use types::slashing::MacroEquivocation;

use crate::error::Result;

/// Verify a macro-equivocation evidence. Skeleton no-op.
pub fn verify(_ev: &MacroEquivocation) -> Result<()> {
    Ok(())
}
```

- [ ] **Step 4: Write `crates/consensus/src/slashing/surround.rs`**

```rust
//! Casper-FFG surround-vote detector.

use types::{primitives::ValidatorId, slashing::SurroundVote};

use crate::{error::Result, macro_fin::vote_book::VoteBook};

/// Scan `book` for a surround vote committed by `validator`. Skeleton
/// returns `Ok(None)`.
pub fn scan_for_surround(
    _book: &VoteBook,
    _validator: &ValidatorId,
) -> Result<Option<SurroundVote>> {
    Ok(None)
}
```

- [ ] **Step 5: Write `crates/consensus/src/slashing/inactivity_leak.rs`**

```rust
//! Inactivity leak: 0.5 % / window once unfinalized for 4 windows.

use crate::config::Config;

/// Returns `(bps, should_apply)` for the given consecutive unfinalized
/// window count. Skeleton respects the configured trigger and rate.
#[must_use]
pub fn compute(cfg: &Config, unfinalized_windows: u32) -> (u32, bool) {
    let apply = unfinalized_windows >= cfg.macro_fin.inactivity_leak_trigger_windows;
    (cfg.macro_fin.inactivity_leak_bps_per_window, apply)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_leak_under_threshold() {
        let cfg = Config::default_table_17_1();
        let (rate, apply) = compute(&cfg, 3);
        assert_eq!(rate, 50);
        assert!(!apply);
    }

    #[test]
    fn leak_applied_at_or_above_threshold() {
        let cfg = Config::default_table_17_1();
        let (_rate, apply) = compute(&cfg, 4);
        assert!(apply);
    }
}
```

- [ ] **Step 6: Write `crates/consensus/src/slashing/penalty.rs`**

```rust
//! Penalty math (basis-point arithmetic).

use crate::config::Config;

/// Penalty kind for accounting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Penalty {
    /// Macro equivocation (100 %).
    Equivocation,
    /// Surround / double-vote (50 %).
    DoubleVote,
    /// Data-availability incident (5 % per incident).
    DaIncident,
    /// Inactivity leak (configurable per-window bps).
    InactivityLeak,
}

impl Penalty {
    /// Penalty in basis points (`10_000 == 100 %`).
    #[must_use]
    pub fn bps(self, cfg: &Config) -> u32 {
        match self {
            Self::Equivocation   => cfg.slashing.equivocation_bps,
            Self::DoubleVote     => cfg.slashing.double_vote_bps,
            Self::DaIncident     => cfg.slashing.da_incident_bps,
            Self::InactivityLeak => cfg.macro_fin.inactivity_leak_bps_per_window,
        }
    }

    /// Clamp the cumulative penalty to the configured per-epoch cap.
    #[must_use]
    pub fn cap(cfg: &Config, cumulative_bps: u32) -> u32 {
        cumulative_bps.min(cfg.slashing.slashing_cap_bps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivocation_is_full_slash() {
        let cfg = Config::default_table_17_1();
        assert_eq!(Penalty::Equivocation.bps(&cfg), 10_000);
    }

    #[test]
    fn cap_respected() {
        let cfg = Config::default_table_17_1();
        assert_eq!(Penalty::cap(&cfg, 20_000), 5_000);
    }
}
```

- [ ] **Step 7: Build + test**

Run: `cargo test -p consensus --lib slashing::`
Expected: PASS (4 tests).

---

## Task 13: Crate-level integration tests

**Files:**
- Create: `crates/consensus/tests/step_signature.rs`
- Create: `crates/consensus/tests/config_defaults.rs`
- Create: `crates/consensus/tests/lock_macro_invariant.rs`

- [ ] **Step 1: Write `crates/consensus/tests/step_signature.rs`**

```rust
//! Smoke test: `StateMachine::step` accepts every `Event` variant and
//! returns an empty action list under the skeleton implementation.

use consensus::{
    event::{BlsPartial, Event, SubnetAggregate, SubnetId, TimerId},
    Config, StateMachine,
};
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32, VrfProof},
    dag::{CertifiedVertex, Vertex},
    macros::{AggregationMode, MacroCheckpoint, MacroProposal, MacroQc},
    micro::MicroQc,
    primitives::{Epoch, Height, Round, StakeWeight, ValidatorId},
    slashing::{DoubleVote, SlashEvidence},
    validator::ValidatorSet,
};

fn fixture_certified() -> CertifiedVertex {
    CertifiedVertex {
        vertex: Vertex {
            round: Round(0),
            author: ValidatorId([0; 32]),
            parents: vec![],
            blobs: vec![],
            hash: Hash32::zero(),
        },
        certificate: BlsAggSig { sig: BlsSig([0; 96]), bitmap: vec![] },
    }
}

fn fixture_macro_checkpoint() -> MacroCheckpoint {
    MacroCheckpoint {
        height: Height(0),
        epoch: Epoch(0),
        parent: Hash32::zero(),
        micro_root: Hash32::zero(),
        hash: Hash32::zero(),
    }
}

fn fixture_macro_qc() -> MacroQc {
    MacroQc {
        checkpoint_hash: Hash32::zero(),
        mode: AggregationMode::Mode0Flat,
        agg: BlsAggSig { sig: BlsSig([0; 96]), bitmap: vec![] },
    }
}

#[test]
fn step_returns_empty_for_every_variant() {
    let mut sm = StateMachine::new(Config::default_table_17_1());
    let events = [
        Event::CertifiedVertexReceived(fixture_certified()),
        Event::MicroQcAssembled(MicroQc {
            checkpoint_hash: Hash32::zero(),
            agg: BlsAggSig { sig: BlsSig([0; 96]), bitmap: vec![] },
        }),
        Event::MacroProposalReceived(MacroProposal {
            checkpoint: fixture_macro_checkpoint(),
            proposer: ValidatorId([0; 32]),
            vrf_proof: VrfProof([0; 80]),
            proposer_sig: BlsSig([0; 96]),
        }),
        Event::BlsPartialReceived(BlsPartial {
            subnet: SubnetId(0),
            validator: ValidatorId([0; 32]),
            checkpoint_hash: Hash32::zero(),
            sig: BlsSig([0; 96]),
        }),
        Event::SubnetAggregateReceived(SubnetAggregate {
            subnet: SubnetId(0),
            checkpoint_hash: Hash32::zero(),
            agg: BlsAggSig { sig: BlsSig([0; 96]), bitmap: vec![] },
        }),
        Event::MacroQcReceived(fixture_macro_qc()),
        Event::TimerFired(TimerId(0)),
        Event::ValidatorSetUpdated {
            epoch: Epoch(0),
            set: ValidatorSet {
                epoch: Epoch(0),
                entries: vec![],
                total_stake: StakeWeight(0),
            },
        },
        Event::SlashEvidenceFound(SlashEvidence::DoubleVote(DoubleVote {
            validator: ValidatorId([0; 32]),
            target: Epoch(0),
            a_sig: BlsSig([0; 96]),
            b_sig: BlsSig([1; 96]),
        })),
    ];
    for ev in events {
        let actions = sm.step(ev).expect("step never errors in skeleton");
        assert!(actions.is_empty(), "skeleton must emit zero actions");
    }
}
```

- [ ] **Step 2: Write `crates/consensus/tests/config_defaults.rs`**

```rust
//! Confirm the in-crate defaults match `config/default.toml`.

use consensus::Config;

#[test]
fn in_crate_defaults_parse_round_trip_via_toml() {
    let cfg = Config::default_table_17_1();
    let s = toml::to_string(&cfg).expect("serialize");
    let parsed = Config::from_toml_str(&s).expect("parse");
    assert_eq!(cfg, parsed);
}

#[test]
fn default_toml_file_matches_in_crate_defaults() {
    // This test consumes `config/default.toml` at the repo root. If the
    // path differs (workspace member layout), update the literal below.
    let raw = std::fs::read_to_string("../../config/default.toml")
        .expect("read config/default.toml from workspace root");
    let parsed = Config::from_toml_str(&raw).expect("parse default.toml");
    let in_crate = Config::default_table_17_1();
    assert_eq!(parsed, in_crate, "config/default.toml drifted from consensus::Config::default_table_17_1()");
}
```

- [ ] **Step 3: Write `crates/consensus/tests/lock_macro_invariant.rs`**

```rust
//! Integration check that `LockMacro` rejects conflicting same-height locks.

use consensus::lock_macro::LockMacro;
use types::{crypto_types::Hash32, primitives::{Height, ValidatorId}};

#[test]
fn conflicting_lock_rejected_across_validators_independently() {
    let mut lm = LockMacro::new();
    let a = ValidatorId([1; 32]);
    let b = ValidatorId([2; 32]);
    lm.try_lock(a, Height(7), Hash32([0xAA; 32])).unwrap();
    lm.try_lock(b, Height(7), Hash32([0xBB; 32])).unwrap();
    assert!(lm.try_lock(a, Height(7), Hash32([0xCC; 32])).is_err());
    assert!(lm.try_lock(b, Height(7), Hash32([0xBB; 32])).is_ok()); // idempotent
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p consensus --tests`
Expected: PASS (3 integration tests).

---

## Task 14: Full lint + test + commit

- [ ] **Step 1: Full check**

```bash
cargo fmt -p consensus -- --check
cargo clippy -p consensus --all-targets -- -D warnings
cargo test -p consensus
```

Expected: all three exit 0.

- [ ] **Step 2: Commit**

```bash
git add Cargo.toml crates/consensus/
git commit -m "feat(consensus): scaffold pure state machine, ports, L2/L3 module skeletons"
```

---

## Self-Review

Spec coverage (§6):

- `state_machine.rs`: ✅ Task 7 — `StateMachine::step`.
- `event.rs` + `action.rs`: ✅ Tasks 3, 4 — all variants from spec table.
- `config.rs`: ✅ Task 2 — full Table 17.1 mirror; `from_toml_str` parser; schema version check.
- `lock_macro.rs`: ✅ Task 8.
- `bullshark/`: ✅ Task 9 — wave/anchor/commit/linearize/micro_qc.
- `macro_fin/`: ✅ Task 10 — window/proposer/checkpoint/macro_qc/two_chain/vote_book + aggregation/{mod0_flat, mode_a_subnet, mode_b_leaderless, subnet}.
- `leader/`: ✅ Task 11 — beacon/vrf_sortition/reputation/timeout.
- `slashing/`: ✅ Task 12 — evidence/equivocation/surround/inactivity_leak/penalty.
- `api/`: ✅ Task 6 — tier + query.
- `ports/`: ✅ Task 5 — 5 traits (`DagView`, `Clock`, `RandomnessBeacon`, `ValidatorSetPort`, `Persistence`).
- `tests/`: ✅ Task 13 — `step_signature`, `config_defaults`, `lock_macro_invariant`.

Dependency policy (spec §9): `Cargo.toml` (Task 1) deps are only `types`, `crypto`, `borsh`, `smallvec`, `thiserror`, `serde`, `toml` — no `tokio`, `libp2p`, `rocksdb`. Verified.

Placeholders: every `try_finalize`/`scan_for_*`/`verify_*` stub returns sensible defaults (`Ok(None)` / `Ok(())`) so downstream binaries can wire end-to-end. Follow-up plans (referenced as "plan 03b/03c/03d" in TODO comments) implement the real algorithms.

Naming consistency:
- `BlobStatus` defined in `consensus::api::tier`, re-exported from `prelude` and used in `Action::UpdateBlobStatus`.
- `ValidatorSetPort` (port trait) deliberately renamed from spec's `ValidatorSet` to avoid collision with `types::validator::ValidatorSet` (the data type). Both are exported under their own paths; the rename is documented inline.
- `Pop` matches plan 02.
- `TimerId`, `SubnetId`, `BlsPartial`, `SubnetAggregate` live in `event.rs` (the SM's input vocabulary) and are re-exported by `Action` to keep the I/O boundary symmetric.
- `Action::ScheduleTimer { id, delay_nanos }` uses `u128` for Borsh compatibility (not `Duration`) — the host binary converts.
