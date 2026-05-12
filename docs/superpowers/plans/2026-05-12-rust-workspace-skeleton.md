# LUA-DAG Rust Workspace Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Set up the greenfield Cargo workspace defined in [docs/superpowers/specs/2026-05-11-folder-architecture-design.md](../specs/2026-05-11-folder-architecture-design.md) — 5 library crates, 3 binary crates, every module file the spec mandates, and every public type/trait that forms a cross-crate contract — so that `cargo build --workspace && cargo test --workspace` both succeed and the architecture is ready for L2/L3 feature implementation in subsequent plans.

**Architecture:** Pure deterministic state machine in [crates/consensus/](../../../crates/consensus/) exposes a single `StateMachine::step(Event) -> Vec<Action>` entrypoint and 5 outbound trait ports. Adapters [crates/net/](../../../crates/net/) (libp2p) and [crates/storage/](../../../crates/storage/) (RocksDB) depend on consensus only via those traits. Apps glue everything in [apps/node/](../../../apps/node/) (production), [apps/sim/](../../../apps/sim/) (deterministic adversarial simulator, no I/O), and [apps/cli/](../../../apps/cli/) (ops). [crates/types/](../../../crates/types/) defines on-wire data structures with Borsh canonical codec; [crates/crypto/](../../../crates/crypto/) wraps BLS12-381 (blst) + ECVRF.

**Tech Stack:** Rust 2024 edition (stable), Cargo workspace, Borsh 1.x, blst 0.3, ecvrf, libp2p 0.55, rocksdb 0.23, tokio 1, clap 4, tracing 0.1, prometheus 0.13, criterion 0.5, proptest 1, smallvec 1, thiserror 1.

**Out of scope (handled by later plans):** Bullshark wave/commit algorithm bodies, macro-finality aggregation logic, slashing detector logic, libp2p wire formats, RocksDB schema details, simulator scenarios. This plan creates the *files and public contracts* the spec names; it does **not** implement L2 or L3 algorithms inside them.

---

## File Map

Library crates (under `crates/`):

| Crate | Top-level responsibility | Depends on |
|-------|--------------------------|------------|
| `types` | On-wire structs, Borsh codec, primitive newtypes | — |
| `crypto` | BLS, ECVRF, hash, KDF wrappers | `types` |
| `consensus` | Pure state machine, 5 ports traits, Event/Action | `types`, `crypto` |
| `net` | libp2p gossip/RPC adapter; `bridge.rs` translates Event↔Action | `types`, `consensus` |
| `storage` | RocksDB adapter; impls `consensus::ports::Persistence` | `types`, `consensus` |

Binary crates (under `apps/`):

| Crate | Responsibility | Depends on |
|-------|----------------|------------|
| `node` | Validator production binary; tokio + libp2p + rocksdb glue | `types`, `crypto`, `consensus`, `net`, `storage` |
| `sim` | Deterministic adversarial simulator; no I/O | `types`, `crypto`, `consensus` |
| `cli` | Dev/ops/inspect tool | `types`, `crypto`, `storage` |

Workspace root files: `Cargo.toml` (with `[workspace.dependencies]`), `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, `.gitignore`, `.github/workflows/{ci,audit}.yml`, `tests/`, `benches/`, `fuzz/`, `config/`, `scripts/`, `docs/architecture/`.

Naming notes from spec §11:
- Crate names are bare (`types`, not `lua-dag-types`); `package.publish = false` workspace-wide.
- Use `macros/` (plural) in `types`, and `macro_fin/` in `consensus` — `macro` is a Rust keyword.

---

## Task 1: Workspace root setup

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `rustfmt.toml`
- Create: `clippy.toml`
- Create: `deny.toml`
- Create: `.gitignore`

- [ ] **Step 1: Create workspace `Cargo.toml`**

Write `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
    "crates/types",
    "crates/crypto",
    "crates/consensus",
    "crates/net",
    "crates/storage",
    "apps/node",
    "apps/sim",
    "apps/cli",
]

[workspace.package]
edition = "2024"
rust-version = "1.85"
license = "MIT OR Apache-2.0"
publish = false
authors = ["LUA-DAG Contributors"]
repository = "https://github.com/1hoodlabs/lua-dag-consensus"

[workspace.dependencies]
# Local
types = { path = "crates/types" }
crypto = { path = "crates/crypto" }
consensus = { path = "crates/consensus" }
net = { path = "crates/net" }
storage = { path = "crates/storage" }

# Codec & serde
borsh = { version = "1.5", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Errors & utils
thiserror = "1"
anyhow = "1"
smallvec = { version = "1", features = ["serde"] }
hex = "0.4"

# Crypto
blst = "0.3"
blake3 = "1"
sha2 = "0.10"

# Async / runtime
tokio = { version = "1", features = ["full"] }
futures = "0.3"

# Network
libp2p = { version = "0.55", features = ["gossipsub", "noise", "quic", "tcp", "yamux", "kad", "identify", "tokio", "macros"] }

# Storage
rocksdb = "0.23"

# CLI / config
clap = { version = "4", features = ["derive"] }
toml = "0.8"

# Observability
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
prometheus = "0.13"

# Testing
proptest = "1"
criterion = "0.5"

[profile.release]
lto = "thin"
codegen-units = 1
panic = "abort"

[profile.dev]
opt-level = 0

[profile.bench]
inherits = "release"
debug = true
```

- [ ] **Step 2: Pin toolchain**

Write `rust-toolchain.toml`:

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

- [ ] **Step 3: Add formatter + lint config**

Write `rustfmt.toml`:

```toml
edition = "2024"
max_width = 100
use_field_init_shorthand = true
use_try_shorthand = true
imports_granularity = "Crate"
group_imports = "StdExternalCrate"
```

Write `clippy.toml`:

```toml
msrv = "1.85"
avoid-breaking-exported-api = false
```

- [ ] **Step 4: Add cargo-deny config**

Write `deny.toml`:

```toml
[advisories]
yanked = "deny"
ignore = []

[licenses]
allow = [
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-DFS-2016",
    "Unicode-3.0",
    "Zlib",
    "CC0-1.0",
]
confidence-threshold = 0.93

[bans]
multiple-versions = "warn"
wildcards = "deny"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
```

- [ ] **Step 5: Add `.gitignore`**

Write `.gitignore`:

```
/target
/Cargo.lock.bak
**/*.rs.bk
*.pdb
.DS_Store
.idea/
.vscode/*
!.vscode/settings.json
*.swp
*.swo
```

- [ ] **Step 6: Verify workspace parses**

Run: `cargo metadata --no-deps --format-version 1 > NUL`
Expected: Exits 0 with no output to stderr. (Members don't exist yet → expect errors about missing manifests; that's fine, we'll add them.)

Actually, with members listed but not yet present, `cargo metadata` will fail. Skip verification until Task 2 creates the first crate. Continue.

- [ ] **Step 7: Commit**

```powershell
git add Cargo.toml rust-toolchain.toml rustfmt.toml clippy.toml deny.toml .gitignore
git commit -m "chore: scaffold cargo workspace with toolchain and lint config"
```

---

## Task 2: `types` crate — primitives

**Files:**
- Create: `crates/types/Cargo.toml`
- Create: `crates/types/src/lib.rs`
- Create: `crates/types/src/primitives.rs`
- Create: `crates/types/tests/primitives_basics.rs`

- [ ] **Step 1: Create crate manifest**

Write `crates/types/Cargo.toml`:

```toml
[package]
name = "types"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
borsh.workspace = true
serde.workspace = true
thiserror.workspace = true
hex.workspace = true
smallvec.workspace = true

[dev-dependencies]
proptest.workspace = true
```

- [ ] **Step 2: Write the failing test for primitive newtypes**

Write `crates/types/tests/primitives_basics.rs`:

```rust
use types::primitives::{Epoch, Height, Round, StakeWeight, ValidatorId};

#[test]
fn round_increments_and_orders() {
    let r0 = Round::ZERO;
    let r1 = r0.next();
    assert!(r1 > r0);
    assert_eq!(r1.as_u64(), 1);
}

#[test]
fn height_and_epoch_are_distinct_types() {
    let h = Height::new(10);
    let e = Epoch::new(10);
    assert_eq!(h.as_u64(), e.as_u64());
}

#[test]
fn validator_id_round_trips_through_bytes() {
    let id = ValidatorId::from_bytes([7u8; 32]);
    assert_eq!(id.as_bytes(), &[7u8; 32]);
}

#[test]
fn stake_weight_arithmetic_saturates() {
    let a = StakeWeight::new(100);
    let b = StakeWeight::new(u128::MAX);
    let sum = a.saturating_add(b);
    assert_eq!(sum, StakeWeight::new(u128::MAX));
}
```

- [ ] **Step 3: Run test — expect compile failure**

Run: `cargo test -p types --test primitives_basics`
Expected: FAIL — `types` crate does not yet expose these items.

- [ ] **Step 4: Implement primitives**

Write `crates/types/src/lib.rs`:

```rust
//! Shared on-wire data structures and canonical codec for LUA-DAG.
//!
//! Contains only data definitions — no business logic, no verification.

pub mod primitives;
```

Write `crates/types/src/primitives.rs`:

```rust
use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

macro_rules! u64_newtype {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
            BorshSerialize,
            BorshDeserialize,
        )]
        #[serde(transparent)]
        pub struct $name(u64);

        impl $name {
            pub const ZERO: Self = Self(0);

            pub const fn new(v: u64) -> Self {
                Self(v)
            }

            pub const fn as_u64(self) -> u64 {
                self.0
            }

            pub const fn next(self) -> Self {
                Self(self.0 + 1)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }
    };
}

u64_newtype!(Round);
u64_newtype!(Height);
u64_newtype!(Epoch);
u64_newtype!(Wave);
u64_newtype!(TimerId);

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct StakeWeight(u128);

impl StakeWeight {
    pub const ZERO: Self = Self(0);

    pub const fn new(v: u128) -> Self {
        Self(v)
    }

    pub const fn as_u128(self) -> u128 {
        self.0
    }

    pub fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    pub fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct ValidatorId([u8; 32]);

impl ValidatorId {
    pub const fn from_bytes(b: [u8; 32]) -> Self {
        Self(b)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for ValidatorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ValidatorId({})", hex::encode(self.0))
    }
}

#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct BlobId([u8; 32]);

impl BlobId {
    pub const fn from_bytes(b: [u8; 32]) -> Self {
        Self(b)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for BlobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlobId({})", hex::encode(self.0))
    }
}
```

- [ ] **Step 5: Run test — expect pass**

Run: `cargo test -p types --test primitives_basics`
Expected: PASS — 4 tests pass.

- [ ] **Step 6: Run clippy on the new crate**

Run: `cargo clippy -p types -- -D warnings`
Expected: No warnings.

- [ ] **Step 7: Commit**

```powershell
git add crates/types
git commit -m "feat(types): add primitive newtypes (Round, Height, Epoch, Wave, ValidatorId, BlobId, StakeWeight)"
```

---

## Task 3: `types` crate — crypto types and error

**Files:**
- Create: `crates/types/src/crypto_types.rs`
- Create: `crates/types/src/error.rs`
- Modify: `crates/types/src/lib.rs`

- [ ] **Step 1: Add opaque crypto type wrappers**

Write `crates/types/src/crypto_types.rs`:

```rust
use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct Hash32(pub [u8; 32]);

impl Hash32 {
    pub const ZERO: Self = Self([0u8; 32]);

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for Hash32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash32({})", hex::encode(self.0))
    }
}

/// Compressed BLS12-381 G1 public key (48 bytes).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct BlsPubkey(pub [u8; 48]);

/// Compressed BLS12-381 G2 signature (96 bytes).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct BlsSig(pub [u8; 96]);

/// Aggregated BLS signature with attached signer bitmap.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct BlsAggSig {
    pub sig: BlsSig,
    pub signer_bitmap: Vec<u8>,
}

/// ECVRF Edwards25519 proof (80 bytes per RFC 9381).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct VrfProof(pub [u8; 80]);

/// Proof-of-possession signature (rogue-key defense).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PoP(pub BlsSig);

impl fmt::Debug for BlsPubkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlsPubkey({})", hex::encode(self.0))
    }
}

impl fmt::Debug for BlsSig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlsSig({})", hex::encode(self.0))
    }
}

impl fmt::Debug for VrfProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VrfProof({})", hex::encode(self.0))
    }
}

impl fmt::Debug for BlsAggSig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlsAggSig")
            .field("sig", &self.sig)
            .field("signers_len", &self.signer_bitmap.len())
            .finish()
    }
}

impl fmt::Debug for PoP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PoP({:?})", self.0)
    }
}
```

- [ ] **Step 2: Add error type**

Write `crates/types/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TypesError {
    #[error("invalid length: expected {expected}, got {got}")]
    InvalidLength { expected: usize, got: usize },

    #[error("borsh decode failed: {0}")]
    BorshDecode(#[from] std::io::Error),

    #[error("invalid hex: {0}")]
    Hex(#[from] hex::FromHexError),
}
```

- [ ] **Step 3: Wire the new modules**

Edit `crates/types/src/lib.rs` — replace contents with:

```rust
//! Shared on-wire data structures and canonical codec for LUA-DAG.
//!
//! Contains only data definitions — no business logic, no verification.

pub mod crypto_types;
pub mod error;
pub mod primitives;

pub use error::TypesError;
```

- [ ] **Step 4: Verify build**

Run: `cargo build -p types`
Expected: Builds clean.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy -p types -- -D warnings`
Expected: No warnings.

- [ ] **Step 6: Commit**

```powershell
git add crates/types
git commit -m "feat(types): add crypto_types (Hash32, BlsPubkey/Sig/AggSig, VrfProof, PoP) and TypesError"
```

---

## Task 4: `types` crate — canonical codec roundtrip

**Files:**
- Create: `crates/types/src/codec/mod.rs`
- Create: `crates/types/src/codec/borsh_impl.rs`
- Create: `crates/types/tests/codec_roundtrip.rs`
- Modify: `crates/types/src/lib.rs`

- [ ] **Step 1: Write the failing roundtrip test**

Write `crates/types/tests/codec_roundtrip.rs`:

```rust
use types::codec::{decode, encode};
use types::crypto_types::Hash32;
use types::primitives::{Epoch, Round, StakeWeight, ValidatorId};

#[test]
fn roundtrip_round() {
    let r = Round::new(42);
    let bytes = encode(&r).unwrap();
    let back: Round = decode(&bytes).unwrap();
    assert_eq!(r, back);
}

#[test]
fn roundtrip_stake_weight() {
    let s = StakeWeight::new(1_000_000_000_000_000);
    let bytes = encode(&s).unwrap();
    let back: StakeWeight = decode(&bytes).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roundtrip_validator_id() {
    let v = ValidatorId::from_bytes([0xAB; 32]);
    let bytes = encode(&v).unwrap();
    let back: ValidatorId = decode(&bytes).unwrap();
    assert_eq!(v, back);
}

#[test]
fn encoding_is_deterministic() {
    let h = Hash32([0xCD; 32]);
    let a = encode(&h).unwrap();
    let b = encode(&h).unwrap();
    assert_eq!(a, b);
}

#[test]
fn round_encoding_is_compact_little_endian_u64() {
    let r = Round::new(1);
    let bytes = encode(&r).unwrap();
    // u64 little-endian = 8 bytes; first byte is least significant.
    assert_eq!(bytes, vec![1, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn epoch_roundtrip_through_borsh() {
    let e = Epoch::new(u64::MAX);
    let bytes = encode(&e).unwrap();
    let back: Epoch = decode(&bytes).unwrap();
    assert_eq!(e, back);
}
```

- [ ] **Step 2: Run test — expect compile failure**

Run: `cargo test -p types --test codec_roundtrip`
Expected: FAIL — `types::codec` module does not exist.

- [ ] **Step 3: Implement codec**

Write `crates/types/src/codec/mod.rs`:

```rust
//! Canonical (deterministic) serialization for on-wire and on-disk values.
//!
//! Currently backed by Borsh. The public surface is the two free functions
//! `encode` and `decode` so the underlying library can be swapped without
//! touching call sites.

pub mod borsh_impl;

pub use borsh_impl::{decode, encode};
```

Write `crates/types/src/codec/borsh_impl.rs`:

```rust
use borsh::{BorshDeserialize, BorshSerialize};

use crate::error::TypesError;

pub fn encode<T: BorshSerialize>(value: &T) -> Result<Vec<u8>, TypesError> {
    borsh::to_vec(value).map_err(TypesError::BorshDecode)
}

pub fn decode<T: BorshDeserialize>(bytes: &[u8]) -> Result<T, TypesError> {
    T::try_from_slice(bytes).map_err(TypesError::BorshDecode)
}
```

- [ ] **Step 4: Re-export codec from lib.rs**

Edit `crates/types/src/lib.rs` — replace the module list with:

```rust
//! Shared on-wire data structures and canonical codec for LUA-DAG.

pub mod codec;
pub mod crypto_types;
pub mod error;
pub mod primitives;

pub use error::TypesError;
```

- [ ] **Step 5: Run test — expect pass**

Run: `cargo test -p types --test codec_roundtrip`
Expected: PASS — 6 tests pass, including the deterministic encoding check.

- [ ] **Step 6: Commit**

```powershell
git add crates/types
git commit -m "feat(types): add Borsh canonical codec with deterministic roundtrip tests"
```

---

## Task 5: `types` crate — domain structs (dag, micro, macros, validator, slashing)

**Files:**
- Create: `crates/types/src/dag/mod.rs`
- Create: `crates/types/src/dag/vertex.rs`
- Create: `crates/types/src/dag/certified.rs`
- Create: `crates/types/src/dag/refs.rs`
- Create: `crates/types/src/micro/mod.rs`
- Create: `crates/types/src/micro/checkpoint.rs`
- Create: `crates/types/src/micro/qc.rs`
- Create: `crates/types/src/macros/mod.rs`
- Create: `crates/types/src/macros/checkpoint.rs`
- Create: `crates/types/src/macros/qc.rs`
- Create: `crates/types/src/macros/header.rs`
- Create: `crates/types/src/macros/proposal.rs`
- Create: `crates/types/src/validator/mod.rs`
- Create: `crates/types/src/validator/identity.rs`
- Create: `crates/types/src/validator/set.rs`
- Create: `crates/types/src/validator/dkg.rs`
- Create: `crates/types/src/slashing.rs`
- Modify: `crates/types/src/lib.rs`

- [ ] **Step 1: DAG types**

Write `crates/types/src/dag/mod.rs`:

```rust
pub mod certified;
pub mod refs;
pub mod vertex;

pub use certified::CertifiedVertex;
pub use refs::{BlobRef, ChunkRef};
pub use vertex::Vertex;
```

Write `crates/types/src/dag/vertex.rs`:

```rust
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::crypto_types::Hash32;
use crate::dag::refs::BlobRef;
use crate::primitives::{Round, ValidatorId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Vertex {
    pub author: ValidatorId,
    pub round: Round,
    pub parents: Vec<Hash32>,
    pub payload: Vec<BlobRef>,
}
```

Write `crates/types/src/dag/certified.rs`:

```rust
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::crypto_types::{BlsAggSig, Hash32};
use crate::dag::vertex::Vertex;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CertifiedVertex {
    pub vertex: Vertex,
    pub hash: Hash32,
    pub cert: BlsAggSig,
}
```

Write `crates/types/src/dag/refs.rs`:

```rust
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::crypto_types::Hash32;
use crate::primitives::BlobId;

/// Opaque pointer to an availability-DAG blob. Consensus consumes this read-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct BlobRef {
    pub blob_id: BlobId,
    pub commitment: Hash32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ChunkRef {
    pub blob_id: BlobId,
    pub chunk_index: u32,
    pub commitment: Hash32,
}
```

- [ ] **Step 2: Micro types**

Write `crates/types/src/micro/mod.rs`:

```rust
pub mod checkpoint;
pub mod qc;

pub use checkpoint::MicroCheckpoint;
pub use qc::{MicroQc, MicroVote};
```

Write `crates/types/src/micro/checkpoint.rs`:

```rust
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::crypto_types::Hash32;
use crate::primitives::Round;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MicroCheckpoint {
    pub round: Round,
    pub anchor: Hash32,
    pub committed: Vec<Hash32>,
}
```

Write `crates/types/src/micro/qc.rs`:

```rust
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::crypto_types::{BlsAggSig, BlsSig, Hash32};
use crate::primitives::{Round, ValidatorId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MicroVote {
    pub voter: ValidatorId,
    pub round: Round,
    pub micro_root: Hash32,
    pub sig: BlsSig,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MicroQc {
    pub round: Round,
    pub micro_root: Hash32,
    pub agg: BlsAggSig,
}
```

- [ ] **Step 3: Macro types (folder name `macros/` plural; `macro` is a Rust keyword)**

Write `crates/types/src/macros/mod.rs`:

```rust
//! Macro-finality (L3) on-wire types.
//!
//! Folder name is `macros` (plural) because `macro` is a Rust keyword.

pub mod checkpoint;
pub mod header;
pub mod proposal;
pub mod qc;

pub use checkpoint::MacroCheckpoint;
pub use header::MacroHeader;
pub use proposal::MacroProposal;
pub use qc::{BlsPartial, MacroQc, SubnetAggregate};
```

Write `crates/types/src/macros/checkpoint.rs`:

```rust
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::crypto_types::Hash32;
use crate::primitives::{Epoch, Height};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MacroCheckpoint {
    pub height: Height,
    pub epoch: Epoch,
    pub micro_root: Hash32,
    pub parent: Hash32,
    pub state_root: Hash32,
}
```

Write `crates/types/src/macros/qc.rs`:

```rust
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::crypto_types::{BlsAggSig, BlsSig, Hash32};
use crate::primitives::{Epoch, Height, ValidatorId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct BlsPartial {
    pub signer: ValidatorId,
    pub subnet: u32,
    pub height: Height,
    pub macro_root: Hash32,
    pub sig: BlsSig,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct SubnetAggregate {
    pub subnet: u32,
    pub height: Height,
    pub macro_root: Hash32,
    pub agg: BlsAggSig,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MacroQc {
    pub height: Height,
    pub epoch: Epoch,
    pub macro_root: Hash32,
    pub agg: BlsAggSig,
}
```

Write `crates/types/src/macros/header.rs`:

```rust
//! Forward-compatible header for light-client sync committee (whitepaper Ch. 10.1).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::crypto_types::Hash32;
use crate::macros::qc::MacroQc;
use crate::primitives::{Epoch, Height};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MacroHeader {
    pub height: Height,
    pub epoch: Epoch,
    pub parent: Hash32,
    pub state_root: Hash32,
    pub qc: MacroQc,
}
```

Write `crates/types/src/macros/proposal.rs`:

```rust
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::crypto_types::BlsSig;
use crate::macros::checkpoint::MacroCheckpoint;
use crate::primitives::ValidatorId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MacroProposal {
    pub proposer: ValidatorId,
    pub checkpoint: MacroCheckpoint,
    pub sig: BlsSig,
}
```

- [ ] **Step 4: Validator types**

Write `crates/types/src/validator/mod.rs`:

```rust
pub mod dkg;
pub mod identity;
pub mod set;

pub use dkg::DkgCommitment;
pub use identity::ValidatorIdentity;
pub use set::ValidatorSetSnapshot;
```

Write `crates/types/src/validator/identity.rs`:

```rust
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::crypto_types::{BlsPubkey, PoP};
use crate::primitives::ValidatorId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ValidatorIdentity {
    pub id: ValidatorId,
    pub bls_pubkey: BlsPubkey,
    pub pop: PoP,
    /// Anti-Sybil diversity tag: Autonomous System Number.
    pub asn: u32,
    /// Cloud provider tag (free-form, registered out-of-band).
    pub cloud: String,
    /// ISO 3166-1 alpha-2 region.
    pub region: [u8; 2],
}
```

Write `crates/types/src/validator/set.rs`:

```rust
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::primitives::{Epoch, StakeWeight, ValidatorId};
use crate::validator::identity::ValidatorIdentity;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ValidatorSetSnapshot {
    pub epoch: Epoch,
    pub members: Vec<ValidatorIdentity>,
    pub weights: Vec<(ValidatorId, StakeWeight)>,
    pub total_weight: StakeWeight,
}
```

Write `crates/types/src/validator/dkg.rs`:

```rust
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::crypto_types::{BlsPubkey, Hash32};
use crate::primitives::Epoch;

/// Skeleton commitment for distributed key generation (opt-in, whitepaper Ch. 15.1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct DkgCommitment {
    pub epoch: Epoch,
    pub group_pubkey: BlsPubkey,
    pub transcript_hash: Hash32,
}
```

- [ ] **Step 5: Slashing evidence**

Write `crates/types/src/slashing.rs`:

```rust
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::macros::proposal::MacroProposal;
use crate::macros::qc::BlsPartial;
use crate::micro::qc::MicroVote;
use crate::primitives::ValidatorId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum SlashEvidence {
    /// Two conflicting macro proposals from the same proposer at the same height.
    MacroEquivocation {
        offender: ValidatorId,
        a: MacroProposal,
        b: MacroProposal,
    },
    /// Casper-FFG surround vote.
    SurroundVote {
        offender: ValidatorId,
        outer: BlsPartial,
        inner: BlsPartial,
    },
    /// Two conflicting votes for the same height.
    DoubleVote {
        offender: ValidatorId,
        a: BlsPartial,
        b: BlsPartial,
    },
    /// Two conflicting micro-votes in the same round.
    MicroEquivocation {
        offender: ValidatorId,
        a: MicroVote,
        b: MicroVote,
    },
}
```

- [ ] **Step 6: Wire up lib.rs**

Replace `crates/types/src/lib.rs`:

```rust
//! Shared on-wire data structures and canonical codec for LUA-DAG.
//!
//! Contains only data definitions — no business logic, no verification.

pub mod codec;
pub mod crypto_types;
pub mod dag;
pub mod error;
pub mod macros;
pub mod micro;
pub mod primitives;
pub mod slashing;
pub mod validator;

pub use error::TypesError;
```

- [ ] **Step 7: Verify build + tests**

Run: `cargo build -p types && cargo test -p types`
Expected: All tests pass; no warnings.

- [ ] **Step 8: Commit**

```powershell
git add crates/types
git commit -m "feat(types): add dag, micro, macros, validator, slashing data types"
```

---

## Task 6: `crypto` crate — hash with domain separation

**Files:**
- Create: `crates/crypto/Cargo.toml`
- Create: `crates/crypto/src/lib.rs`
- Create: `crates/crypto/src/hash.rs`
- Create: `crates/crypto/src/error.rs`
- Create: `crates/crypto/tests/hash_domain_separation.rs`

- [ ] **Step 1: Crate manifest**

Write `crates/crypto/Cargo.toml`:

```toml
[package]
name = "crypto"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
types.workspace = true
blst.workspace = true
blake3.workspace = true
sha2.workspace = true
thiserror.workspace = true
```

- [ ] **Step 2: Write the failing test**

Write `crates/crypto/tests/hash_domain_separation.rs`:

```rust
use crypto::hash::{Domain, blake3_tagged};

#[test]
fn same_input_different_domain_differs() {
    let payload = b"hello";
    let a = blake3_tagged(Domain::Vertex, payload);
    let b = blake3_tagged(Domain::MicroVote, payload);
    assert_ne!(a, b, "domain separation must change the output");
}

#[test]
fn hash_is_32_bytes_and_deterministic() {
    let payload = b"deterministic";
    let a = blake3_tagged(Domain::MacroCheckpoint, payload);
    let b = blake3_tagged(Domain::MacroCheckpoint, payload);
    assert_eq!(a, b);
    assert_eq!(a.as_bytes().len(), 32);
}
```

- [ ] **Step 3: Run test — expect compile failure**

Run: `cargo test -p crypto --test hash_domain_separation`
Expected: FAIL — `crypto` does not exist yet.

- [ ] **Step 4: Implement hash**

Write `crates/crypto/src/lib.rs`:

```rust
//! Cryptographic primitive wrappers for LUA-DAG.

pub mod error;
pub mod hash;

pub use error::CryptoError;
```

Write `crates/crypto/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("bls verify failed")]
    BlsVerify,

    #[error("bls aggregate empty input")]
    BlsAggregateEmpty,

    #[error("invalid bls encoding")]
    BlsEncoding,

    #[error("vrf verify failed")]
    VrfVerify,

    #[error("vrf decode failed")]
    VrfDecode,
}
```

Write `crates/crypto/src/hash.rs`:

```rust
use types::crypto_types::Hash32;

/// Domain separation tags. Each value is a unique constant string written to
/// the hash before the payload so the same bytes never hash the same way
/// across two protocol-level uses.
#[derive(Clone, Copy, Debug)]
pub enum Domain {
    Vertex,
    MicroVote,
    MicroQc,
    MacroProposal,
    MacroCheckpoint,
    MacroQc,
    BeaconChain,
    SubnetAssign,
    SlashEvidence,
    ValidatorIdentity,
}

impl Domain {
    pub const fn tag(self) -> &'static [u8] {
        match self {
            Domain::Vertex => b"LUADAG-VERTEX-v1",
            Domain::MicroVote => b"LUADAG-MICROVOTE-v1",
            Domain::MicroQc => b"LUADAG-MICROQC-v1",
            Domain::MacroProposal => b"LUADAG-MACROPROP-v1",
            Domain::MacroCheckpoint => b"LUADAG-MACROCHK-v1",
            Domain::MacroQc => b"LUADAG-MACROQC-v1",
            Domain::BeaconChain => b"LUADAG-BEACON-v1",
            Domain::SubnetAssign => b"LUADAG-SUBNET-v1",
            Domain::SlashEvidence => b"LUADAG-SLASH-v1",
            Domain::ValidatorIdentity => b"LUADAG-VALID-v1",
        }
    }
}

pub fn blake3_tagged(domain: Domain, payload: &[u8]) -> Hash32 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain.tag());
    hasher.update(&[0x00]); // separator between tag and payload
    hasher.update(payload);
    let out = hasher.finalize();
    Hash32(*out.as_bytes())
}

pub fn sha256(payload: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(payload);
    let out = h.finalize();
    out.into()
}
```

- [ ] **Step 5: Run test — expect pass**

Run: `cargo test -p crypto --test hash_domain_separation`
Expected: PASS — both tests pass.

- [ ] **Step 6: Clippy clean**

Run: `cargo clippy -p crypto -- -D warnings`
Expected: No warnings.

- [ ] **Step 7: Commit**

```powershell
git add crates/crypto
git commit -m "feat(crypto): add blake3 hash with domain separation tags"
```

---

## Task 7: `crypto` crate — BLS module skeleton

**Files:**
- Create: `crates/crypto/src/bls/mod.rs`
- Create: `crates/crypto/src/bls/keys.rs`
- Create: `crates/crypto/src/bls/sign.rs`
- Create: `crates/crypto/src/bls/aggregate.rs`
- Create: `crates/crypto/src/bls/bitmap.rs`
- Modify: `crates/crypto/src/lib.rs`

- [ ] **Step 1: BLS module tree**

Write `crates/crypto/src/bls/mod.rs`:

```rust
//! BLS12-381 (minimal-pubkey-size: pubkeys in G1, sigs in G2) wrapping `blst`.
//!
//! All public API operates on the opaque types defined in `types::crypto_types`
//! so callers never depend on the underlying library directly.

pub mod aggregate;
pub mod bitmap;
pub mod keys;
pub mod sign;
```

Write `crates/crypto/src/bls/keys.rs`:

```rust
use types::crypto_types::{BlsPubkey, PoP};

/// Domain-separation tag for proof-of-possession (rogue-key defense).
pub const POP_DST: &[u8] = b"LUADAG-BLS-POP-v1";

/// Domain-separation tag for ordinary signatures.
pub const SIG_DST: &[u8] = b"LUADAG-BLS-SIG-v1";

/// Opaque BLS secret key. Holds 32 bytes; never serialized to disk in this skeleton.
#[derive(Clone)]
pub struct SecretKey(pub(crate) [u8; 32]);

impl SecretKey {
    pub fn from_bytes(b: [u8; 32]) -> Self {
        Self(b)
    }

    pub fn public_key(&self) -> BlsPubkey {
        // Implementation will use blst::SecretKey::from_bytes + sk_to_pk; deferred to
        // BLS implementation plan.
        let _ = self;
        BlsPubkey([0u8; 48])
    }

    pub fn proof_of_possession(&self) -> PoP {
        let _ = self;
        PoP(types::crypto_types::BlsSig([0u8; 96]))
    }
}
```

Write `crates/crypto/src/bls/sign.rs`:

```rust
use types::crypto_types::{BlsPubkey, BlsSig};

use crate::bls::keys::SecretKey;
use crate::error::CryptoError;

pub fn sign(_sk: &SecretKey, _message: &[u8]) -> BlsSig {
    // Implementation deferred to BLS implementation plan.
    BlsSig([0u8; 96])
}

pub fn verify(_pk: &BlsPubkey, _message: &[u8], _sig: &BlsSig) -> Result<(), CryptoError> {
    // Implementation deferred to BLS implementation plan.
    Err(CryptoError::BlsVerify)
}
```

Write `crates/crypto/src/bls/aggregate.rs`:

```rust
use types::crypto_types::{BlsAggSig, BlsPubkey, BlsSig};

use crate::error::CryptoError;

pub fn aggregate_signatures(sigs: &[BlsSig]) -> Result<BlsSig, CryptoError> {
    if sigs.is_empty() {
        return Err(CryptoError::BlsAggregateEmpty);
    }
    // Implementation deferred to BLS implementation plan.
    Ok(BlsSig([0u8; 96]))
}

pub fn verify_aggregate(
    _pubkeys: &[BlsPubkey],
    _message: &[u8],
    _agg: &BlsAggSig,
) -> Result<(), CryptoError> {
    Err(CryptoError::BlsVerify)
}
```

Write `crates/crypto/src/bls/bitmap.rs`:

```rust
/// Helper for the signer-bitmap inside `BlsAggSig`. One bit per validator
/// in the canonical validator-set ordering for the epoch.
#[derive(Clone, Debug, Default)]
pub struct SignerBitmap {
    bytes: Vec<u8>,
    bit_len: usize,
}

impl SignerBitmap {
    pub fn with_capacity(bits: usize) -> Self {
        Self {
            bytes: vec![0u8; bits.div_ceil(8)],
            bit_len: bits,
        }
    }

    pub fn set(&mut self, index: usize) {
        assert!(index < self.bit_len, "index out of range");
        self.bytes[index / 8] |= 1 << (index % 8);
    }

    pub fn is_set(&self, index: usize) -> bool {
        if index >= self.bit_len {
            return false;
        }
        (self.bytes[index / 8] >> (index % 8)) & 1 == 1
    }

    pub fn count_ones(&self) -> usize {
        self.bytes.iter().map(|b| b.count_ones() as usize).sum()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_check_bits() {
        let mut bm = SignerBitmap::with_capacity(20);
        bm.set(0);
        bm.set(7);
        bm.set(19);
        assert!(bm.is_set(0));
        assert!(bm.is_set(7));
        assert!(bm.is_set(19));
        assert!(!bm.is_set(1));
        assert_eq!(bm.count_ones(), 3);
    }
}
```

- [ ] **Step 2: Wire module in lib.rs**

Replace `crates/crypto/src/lib.rs`:

```rust
//! Cryptographic primitive wrappers for LUA-DAG.

pub mod bls;
pub mod error;
pub mod hash;

pub use error::CryptoError;
```

- [ ] **Step 3: Verify build + run tests**

Run: `cargo build -p crypto && cargo test -p crypto`
Expected: Builds clean; bitmap test passes.

- [ ] **Step 4: Commit**

```powershell
git add crates/crypto
git commit -m "feat(crypto): add bls module skeleton (keys, sign, aggregate, signer bitmap)"
```

---

## Task 8: `crypto` crate — VRF, KDF, DKG skeletons

**Files:**
- Create: `crates/crypto/src/vrf/mod.rs`
- Create: `crates/crypto/src/vrf/ecvrf.rs`
- Create: `crates/crypto/src/vrf/sortition.rs`
- Create: `crates/crypto/src/kdf.rs`
- Create: `crates/crypto/src/dkg/mod.rs`
- Create: `crates/crypto/src/dkg/fingerprint.rs`
- Modify: `crates/crypto/src/lib.rs`
- Modify: `crates/crypto/Cargo.toml` (add `hkdf` dep)

- [ ] **Step 1: VRF module**

Write `crates/crypto/src/vrf/mod.rs`:

```rust
//! ECVRF Edwards25519 (RFC 9381) + stake-weighted sortition.

pub mod ecvrf;
pub mod sortition;
```

Write `crates/crypto/src/vrf/ecvrf.rs`:

```rust
use types::crypto_types::{Hash32, VrfProof};

use crate::error::CryptoError;

/// Opaque VRF secret key (32 bytes). Implementation deferred.
#[derive(Clone)]
pub struct VrfSecretKey(pub [u8; 32]);

/// Opaque VRF public key (32 bytes). Implementation deferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VrfPublicKey(pub [u8; 32]);

pub fn prove(_sk: &VrfSecretKey, _alpha: &[u8]) -> VrfProof {
    VrfProof([0u8; 80])
}

pub fn verify(_pk: &VrfPublicKey, _alpha: &[u8], _proof: &VrfProof) -> Result<Hash32, CryptoError> {
    Err(CryptoError::VrfVerify)
}

/// Deterministically hash a VRF proof to a uniform output for sortition.
pub fn proof_to_hash(_proof: &VrfProof) -> Hash32 {
    Hash32::ZERO
}
```

Write `crates/crypto/src/vrf/sortition.rs`:

```rust
use types::crypto_types::Hash32;
use types::primitives::StakeWeight;

/// Compute the sortition score `y_i · W / (w_i · rep_i)` per whitepaper Ch. 8.1.
///
/// The validator with the lowest score in a round is the elected anchor.
/// Reputation is in `[0.8, 1.2]` (Shoal); represented here as a fixed-point
/// integer with denominator `REP_DENOMINATOR`.
pub const REP_DENOMINATOR: u128 = 1_000_000;

pub fn score(_y: Hash32, _total_weight: StakeWeight, _w_i: StakeWeight, _rep_i: u128) -> u128 {
    // Implementation deferred to leader election plan (whitepaper Ch. 8.1).
    0
}
```

- [ ] **Step 2: KDF**

Write `crates/crypto/src/kdf.rs`:

```rust
//! HKDF-SHA256 wrapper for beacon chaining and subnet assignment.

use sha2::Sha256;

/// Output 32 bytes from `(salt, ikm, info)` using HKDF-SHA256.
pub fn hkdf_32(salt: &[u8], ikm: &[u8], info: &[u8]) -> [u8; 32] {
    use hkdf::Hkdf;
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut out = [0u8; 32];
    hk.expand(info, &mut out).expect("32 bytes is within HKDF output bound");
    out
}
```

Add `hkdf` to `crates/crypto/Cargo.toml` under `[dependencies]`:

```toml
hkdf = "0.12"
```

- [ ] **Step 3: DKG fingerprint**

Write `crates/crypto/src/dkg/mod.rs`:

```rust
//! DKG ceremony skeleton — opt-in mandatory for new validator activations
//! once governance enables it (whitepaper Ch. 15.1).

pub mod fingerprint;
```

Write `crates/crypto/src/dkg/fingerprint.rs`:

```rust
use types::crypto_types::Hash32;
use types::validator::dkg::DkgCommitment;

use crate::hash::{Domain, blake3_tagged};

pub fn fingerprint(c: &DkgCommitment) -> Hash32 {
    let mut payload = Vec::with_capacity(8 + 48 + 32);
    payload.extend_from_slice(&c.epoch.as_u64().to_le_bytes());
    payload.extend_from_slice(&c.group_pubkey.0);
    payload.extend_from_slice(c.transcript_hash.as_bytes());
    blake3_tagged(Domain::ValidatorIdentity, &payload)
}
```

- [ ] **Step 4: Wire modules**

Replace `crates/crypto/src/lib.rs`:

```rust
//! Cryptographic primitive wrappers for LUA-DAG.

pub mod bls;
pub mod dkg;
pub mod error;
pub mod hash;
pub mod kdf;
pub mod vrf;

pub use error::CryptoError;
```

- [ ] **Step 5: Build + clippy**

Run: `cargo build -p crypto && cargo clippy -p crypto -- -D warnings`
Expected: Builds clean; no warnings.

- [ ] **Step 6: Commit**

```powershell
git add crates/crypto
git commit -m "feat(crypto): add vrf, kdf (hkdf-sha256), and dkg fingerprint skeletons"
```

---

## Task 9: `consensus` crate — scaffold + config

**Files:**
- Create: `crates/consensus/Cargo.toml`
- Create: `crates/consensus/src/lib.rs`
- Create: `crates/consensus/src/prelude.rs`
- Create: `crates/consensus/src/config.rs`
- Create: `crates/consensus/tests/config_defaults.rs`

- [ ] **Step 1: Crate manifest**

Write `crates/consensus/Cargo.toml`:

```toml
[package]
name = "consensus"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
types.workspace = true
crypto.workspace = true
borsh.workspace = true
serde.workspace = true
smallvec.workspace = true
thiserror.workspace = true
tracing.workspace = true

[dev-dependencies]
proptest.workspace = true
```

- [ ] **Step 2: Write the failing config test**

Write `crates/consensus/tests/config_defaults.rs`:

```rust
use consensus::config::ConsensusConfig;

#[test]
fn defaults_match_whitepaper_table_17_1() {
    let c = ConsensusConfig::default();
    assert_eq!(c.round_duration_ms, 250);
    assert_eq!(c.macro_window_w, 8);
    assert_eq!(c.micro_committee_size, 256);
    assert_eq!(c.t_macropropose_ms, 4_000);
    assert_eq!(c.t_subnet_ms, 2_000);
    assert_eq!(c.t_canonicalize_ms, 8_000);
    assert_eq!(c.subnet_flat_threshold, 500);
    assert_eq!(c.subnet_full_threshold, 1_000);
    assert_eq!(c.btc_confirmations_for_final, 6);
}

#[test]
fn reputation_bounds_are_shoal_range() {
    let c = ConsensusConfig::default();
    assert_eq!(c.reputation_min_micro, 800_000);
    assert_eq!(c.reputation_max_micro, 1_200_000);
}
```

- [ ] **Step 3: Run test — expect compile failure**

Run: `cargo test -p consensus --test config_defaults`
Expected: FAIL — `consensus::config` does not exist.

- [ ] **Step 4: Implement config**

Write `crates/consensus/src/lib.rs`:

```rust
//! LUA-DAG consensus state machine — pure, deterministic.
//!
//! Public entry points:
//! - [`config::ConsensusConfig`] — single source of truth for protocol parameters
//!   (whitepaper Table 17.1).
//! - The state machine, event/action types, and outbound trait ports — added
//!   in subsequent tasks.

pub mod config;
pub mod prelude;
```

Write `crates/consensus/src/prelude.rs`:

```rust
//! Convenience re-exports for downstream crates.

pub use crate::config::ConsensusConfig;
```

Write `crates/consensus/src/config.rs`:

```rust
//! Single source of truth for protocol parameters (whitepaper Table 17.1).
//!
//! Each field's doc comment cites the chapter or section where it is defined.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsensusConfig {
    /// Duration of a Bullshark round in milliseconds (Ch. 8.1).
    pub round_duration_ms: u64,

    /// Number of micro-slots per macro-finality window (Ch. 9.1).
    pub macro_window_w: u32,

    /// Number of validators in the micro-committee per epoch (Ch. 8.4).
    pub micro_committee_size: u32,

    /// Timeout before backup macro proposer takes over, in milliseconds (Ch. 9.1).
    pub t_macropropose_ms: u64,

    /// Timeout for subnet aggregation, in milliseconds (Ch. 9.2).
    pub t_subnet_ms: u64,

    /// Timeout for full canonicalization across subnets, in milliseconds (Ch. 9.2).
    pub t_canonicalize_ms: u64,

    /// `Ne` threshold below which aggregation runs in Mode 0 (flat) (Eq. 9.1).
    pub subnet_flat_threshold: u32,

    /// `Ne` threshold above which Mode A activates (Eq. 9.1).
    pub subnet_full_threshold: u32,

    /// Bitcoin confirmation depth used to mark `epoch_finalized` (Ch. 12, placeholder).
    pub btc_confirmations_for_final: u32,

    /// Lower bound on Shoal reputation multiplier, as micro-units (Ch. 7.1).
    /// `800_000` means reputation factor 0.8.
    pub reputation_min_micro: u64,

    /// Upper bound on Shoal reputation multiplier, as micro-units (Ch. 7.1).
    /// `1_200_000` means reputation factor 1.2.
    pub reputation_max_micro: u64,

    /// Slashing percentage for equivocation, in basis points (Ch. 9.4). `10_000` = 100%.
    pub slash_equivocation_bp: u32,

    /// Slashing percentage for double-vote, in basis points (Ch. 9.4). `5_000` = 50%.
    pub slash_double_vote_bp: u32,

    /// Per-window inactivity-leak rate in basis points (Ch. 9.4). `50` = 0.5%.
    pub inactivity_leak_bp_per_window: u32,

    /// Window count of unfinalized macros before inactivity leak begins (Ch. 9.4).
    pub inactivity_leak_threshold_windows: u32,

    /// Hot-storage horizon in rounds (Ch. 7.4).
    pub gc_hot_horizon_rounds: u64,

    /// Warm-storage horizon in rounds (Ch. 7.4).
    pub gc_warm_horizon_rounds: u64,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            round_duration_ms: 250,
            macro_window_w: 8,
            micro_committee_size: 256,
            t_macropropose_ms: 4_000,
            t_subnet_ms: 2_000,
            t_canonicalize_ms: 8_000,
            subnet_flat_threshold: 500,
            subnet_full_threshold: 1_000,
            btc_confirmations_for_final: 6,
            reputation_min_micro: 800_000,
            reputation_max_micro: 1_200_000,
            slash_equivocation_bp: 10_000,
            slash_double_vote_bp: 5_000,
            inactivity_leak_bp_per_window: 50,
            inactivity_leak_threshold_windows: 4,
            gc_hot_horizon_rounds: 200,
            gc_warm_horizon_rounds: 10_000,
        }
    }
}
```

- [ ] **Step 5: Run test — expect pass**

Run: `cargo test -p consensus --test config_defaults`
Expected: PASS — both tests pass.

- [ ] **Step 6: Commit**

```powershell
git add crates/consensus
git commit -m "feat(consensus): add ConsensusConfig with Table 17.1 defaults"
```

---

## Task 10: `consensus` crate — Event and Action enums

**Files:**
- Create: `crates/consensus/src/event.rs`
- Create: `crates/consensus/src/action.rs`
- Modify: `crates/consensus/src/lib.rs`
- Modify: `crates/consensus/src/prelude.rs`

- [ ] **Step 1: Write Event enum**

Write `crates/consensus/src/event.rs`:

```rust
//! Every input into the consensus state machine. All external interaction
//! with consensus crosses this boundary — there is no other entry point.

use std::time::Instant;

use types::crypto_types::Hash32;
use types::dag::CertifiedVertex;
use types::macros::{MacroProposal, qc::{BlsPartial, SubnetAggregate}};
use types::micro::MicroQc;
use types::primitives::{TimerId, ValidatorId};
use types::slashing::SlashEvidence;
use types::validator::ValidatorSetSnapshot;

#[derive(Clone, Debug)]
pub enum Event {
    /// A new certified vertex arrived from the availability DAG (L1).
    CertifiedVertexReceived(CertifiedVertex),

    /// Local Bullshark layer assembled a MicroQC for a wave.
    MicroQcAssembled(MicroQc),

    /// A macro proposal arrived from the network.
    MacroProposalReceived(MacroProposal),

    /// A BLS partial signature arrived (Mode 0 or Mode A).
    BlsPartialReceived(BlsPartial),

    /// A subnet aggregate arrived (Mode A or Mode B).
    SubnetAggregateReceived(SubnetAggregate),

    /// A scheduled timer fired. `id` identifies the timer kind / round / etc.
    TimerFired { id: TimerId, at: Instant },

    /// Validator set rotated (start of a new epoch).
    ValidatorSetUpdated(ValidatorSetSnapshot),

    /// External or local detector reported provable misbehavior.
    SlashEvidenceFound {
        offender: ValidatorId,
        evidence: SlashEvidence,
        evidence_hash: Hash32,
    },
}
```

- [ ] **Step 2: Write Action enum**

Write `crates/consensus/src/action.rs`:

```rust
//! Every output from the consensus state machine. The state machine never
//! performs I/O itself — it only emits actions for the orchestrator to carry out.

use std::time::Instant;

use types::macros::{MacroProposal, qc::{BlsPartial, MacroQc, SubnetAggregate}};
use types::micro::MicroVote;
use types::primitives::{BlobId, TimerId};
use types::slashing::SlashEvidence;

#[derive(Clone, Debug)]
pub enum Action {
    BroadcastMicroVote(MicroVote),
    BroadcastMacroProposal(MacroProposal),
    BroadcastBlsPartial(BlsPartial),
    BroadcastSubnetAggregate(SubnetAggregate),

    ScheduleTimer { id: TimerId, deadline: Instant },
    CancelTimer(TimerId),

    PersistMacroQc(MacroQc),
    EmitSlashEvidence(SlashEvidence),
    UpdateBlobStatus { blob_id: BlobId, status: BlobStatus },
}

/// Lifecycle tier exposed via [`crate::api`] (whitepaper Appendix A).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlobStatus {
    Accepted,
    SoftConfirmed,
    Justified,
    Finalized,
    /// L4 (Bitcoin anchor) — placeholder; remains `Finalized` until L4 lands.
    EpochFinalized,
}
```

- [ ] **Step 3: Wire modules**

Replace `crates/consensus/src/lib.rs`:

```rust
//! LUA-DAG consensus state machine — pure, deterministic.

pub mod action;
pub mod config;
pub mod event;
pub mod prelude;

pub use action::{Action, BlobStatus};
pub use config::ConsensusConfig;
pub use event::Event;
```

Replace `crates/consensus/src/prelude.rs`:

```rust
pub use crate::action::{Action, BlobStatus};
pub use crate::config::ConsensusConfig;
pub use crate::event::Event;
```

- [ ] **Step 4: Build**

Run: `cargo build -p consensus`
Expected: Builds clean.

- [ ] **Step 5: Commit**

```powershell
git add crates/consensus
git commit -m "feat(consensus): add Event and Action enums + BlobStatus lifecycle tier"
```

---

## Task 11: `consensus` crate — ports module (5 traits)

**Files:**
- Create: `crates/consensus/src/ports/mod.rs`
- Create: `crates/consensus/src/ports/dag_view.rs`
- Create: `crates/consensus/src/ports/clock.rs`
- Create: `crates/consensus/src/ports/rng_beacon.rs`
- Create: `crates/consensus/src/ports/validator_set.rs`
- Create: `crates/consensus/src/ports/persistence.rs`
- Modify: `crates/consensus/src/lib.rs`

- [ ] **Step 1: Port traits**

Write `crates/consensus/src/ports/mod.rs`:

```rust
//! Outbound trait ports — the only dependency-inversion seam in the consensus
//! crate. Adapters (`net`, `storage`, `sim`) implement these traits; consensus
//! never imports them.

pub mod clock;
pub mod dag_view;
pub mod persistence;
pub mod rng_beacon;
pub mod validator_set;

pub use clock::Clock;
pub use dag_view::DagView;
pub use persistence::{Persistence, PersistenceError};
pub use rng_beacon::RandomnessBeacon;
pub use validator_set::ValidatorSet;
```

Write `crates/consensus/src/ports/dag_view.rs`:

```rust
use types::crypto_types::Hash32;
use types::dag::CertifiedVertex;
use types::primitives::Round;

/// Read-only view of the availability DAG (L1). Consensus queries this when
/// linearizing waves. The concrete implementation lives in a future `dag` crate.
pub trait DagView {
    fn certified_vertex(&self, hash: Hash32) -> Option<CertifiedVertex>;

    fn parents(&self, hash: Hash32) -> Vec<Hash32>;

    /// All certified vertices whose round equals `round`.
    fn round_vertices(&self, round: Round) -> Vec<Hash32>;
}
```

Write `crates/consensus/src/ports/clock.rs`:

```rust
use std::time::Instant;

/// Source of monotonic time. The simulator implements this with a virtual clock.
pub trait Clock {
    fn now(&self) -> Instant;
}
```

Write `crates/consensus/src/ports/rng_beacon.rs`:

```rust
use types::crypto_types::Hash32;
use types::primitives::Wave;

/// Source of the per-wave randomness beacon `R_w` (whitepaper Eq. 8.1).
///
/// `R_w = H(R_{w-1} ‖ MacroQC)`. The adapter is responsible for chaining;
/// consensus only queries it.
pub trait RandomnessBeacon {
    fn beacon(&self, wave: Wave) -> Hash32;
}
```

Write `crates/consensus/src/ports/validator_set.rs`:

```rust
use types::primitives::{Epoch, StakeWeight, ValidatorId};

pub trait ValidatorSet {
    fn epoch(&self) -> Epoch;
    fn members(&self) -> &[ValidatorId];
    fn weight(&self, validator: ValidatorId) -> StakeWeight;
    fn total_weight(&self) -> StakeWeight;
    fn contains(&self, validator: ValidatorId) -> bool;
}
```

Write `crates/consensus/src/ports/persistence.rs`:

```rust
use thiserror::Error;
use types::macros::qc::MacroQc;
use types::micro::MicroQc;
use types::primitives::Height;
use types::slashing::SlashEvidence;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("persistence backend error: {0}")]
    Backend(String),

    #[error("not found")]
    NotFound,
}

/// Durable storage of consensus-critical artifacts. Implemented by the
/// `storage` crate on RocksDB.
pub trait Persistence {
    fn put_macro_qc(&mut self, qc: &MacroQc) -> Result<(), PersistenceError>;
    fn get_macro_qc(&self, height: Height) -> Result<Option<MacroQc>, PersistenceError>;

    fn put_micro_qc(&mut self, qc: &MicroQc) -> Result<(), PersistenceError>;
    fn append_slash_evidence(&mut self, evidence: &SlashEvidence) -> Result<(), PersistenceError>;
}
```

- [ ] **Step 2: Re-export ports from lib.rs**

Replace `crates/consensus/src/lib.rs`:

```rust
//! LUA-DAG consensus state machine — pure, deterministic.

pub mod action;
pub mod config;
pub mod event;
pub mod ports;
pub mod prelude;

pub use action::{Action, BlobStatus};
pub use config::ConsensusConfig;
pub use event::Event;
```

- [ ] **Step 3: Build + clippy**

Run: `cargo build -p consensus && cargo clippy -p consensus -- -D warnings`
Expected: Builds clean; no warnings.

- [ ] **Step 4: Commit**

```powershell
git add crates/consensus
git commit -m "feat(consensus): add ports module with 5 dependency-inversion traits"
```

---

## Task 12: `consensus` crate — StateMachine + lock_macro

**Files:**
- Create: `crates/consensus/src/state_machine.rs`
- Create: `crates/consensus/src/lock_macro.rs`
- Modify: `crates/consensus/src/lib.rs`

- [ ] **Step 1: StateMachine entry point**

Write `crates/consensus/src/state_machine.rs`:

```rust
//! Single entry point for the consensus state machine.
//!
//! `StateMachine::step(Event) -> SmallVec<[Action; 8]>` is the only public
//! mutation. The body dispatches to layer modules (`bullshark`, `macro_fin`,
//! `slashing`) which are added in later implementation plans.

use smallvec::SmallVec;

use crate::action::Action;
use crate::config::ConsensusConfig;
use crate::event::Event;

pub struct StateMachine {
    #[allow(dead_code)]
    config: ConsensusConfig,
}

impl StateMachine {
    pub fn new(config: ConsensusConfig) -> Self {
        Self { config }
    }

    /// Drive the state machine forward by one event.
    ///
    /// The L2 and L3 dispatch arms are stubs and will be filled in by the
    /// Bullshark and macro-finality implementation plans. Today every event
    /// produces zero actions; the contract is `(Event) -> Vec<Action>`.
    pub fn step(&mut self, event: Event) -> SmallVec<[Action; 8]> {
        let _ = event;
        SmallVec::new()
    }

    pub fn config(&self) -> &ConsensusConfig {
        &self.config
    }
}
```

- [ ] **Step 2: lock_macro signature**

Write `crates/consensus/src/lock_macro.rs`:

```rust
//! Cross-layer invariant `lock_macro` (whitepaper §13.5).
//!
//! A validator that has already voted to justify a macro at height `h` may
//! not vote for any conflicting macro at the same height, even via micro
//! anchors that would re-order vertices around it. The detector returns
//! `false` when the proposed vote would violate the invariant.
//!
//! The body of `permits` is filled in by the macro-finality implementation
//! plan once `vote_book` exists.

use types::crypto_types::Hash32;
use types::primitives::{Height, ValidatorId};

/// Snapshot of a validator's prior votes relevant to lock_macro.
#[derive(Clone, Debug, Default)]
pub struct LockState {
    /// Latest height the validator has justified, and the macro root at that height.
    pub latest_justified: Option<(Height, Hash32)>,
}

/// Does the prior `state` permit `validator` to vote for `macro_root` at `height`?
pub fn permits(
    _state: &LockState,
    _validator: ValidatorId,
    _height: Height,
    _macro_root: Hash32,
) -> bool {
    // Implementation deferred to macro-finality plan.
    true
}
```

- [ ] **Step 3: Wire**

Replace `crates/consensus/src/lib.rs`:

```rust
//! LUA-DAG consensus state machine — pure, deterministic.

pub mod action;
pub mod config;
pub mod event;
pub mod lock_macro;
pub mod ports;
pub mod prelude;
pub mod state_machine;

pub use action::{Action, BlobStatus};
pub use config::ConsensusConfig;
pub use event::Event;
pub use state_machine::StateMachine;
```

- [ ] **Step 4: Verify**

Run: `cargo build -p consensus && cargo clippy -p consensus -- -D warnings`
Expected: Builds clean; no warnings.

- [ ] **Step 5: Commit**

```powershell
git add crates/consensus
git commit -m "feat(consensus): add StateMachine entry point and lock_macro invariant skeleton"
```

---

## Task 13: `consensus` crate — bullshark module tree

**Files:**
- Create: `crates/consensus/src/bullshark/mod.rs`
- Create: `crates/consensus/src/bullshark/wave.rs`
- Create: `crates/consensus/src/bullshark/anchor.rs`
- Create: `crates/consensus/src/bullshark/commit.rs`
- Create: `crates/consensus/src/bullshark/linearize.rs`
- Create: `crates/consensus/src/bullshark/micro_qc.rs`
- Modify: `crates/consensus/src/lib.rs`

- [ ] **Step 1: Module tree**

Write `crates/consensus/src/bullshark/mod.rs`:

```rust
//! L2 — Bullshark micro-ordering (whitepaper Ch. 8).
//!
//! Submodules:
//! - [`wave`] — 4-round wave grouping (rounds `4w..4w+3`)
//! - [`anchor`] — anchor selection via ECVRF sortition (Ch. 8.1)
//! - [`commit`] — shortcut (2-round) + slow-path (4-round) commit (Ch. 8.2)
//! - [`linearize`] — `Closure(Aw)` BFS with hash tie-break (Ch. 8.3)
//! - [`micro_qc`] — MicroQC aggregation `≥ ⌈2/3·C⌉` (Ch. 8.4)
//!
//! Submodules communicate with the rest of consensus only via `state_machine`
//! and `lock_macro`; they do not import `crate::macro_fin`.

pub mod anchor;
pub mod commit;
pub mod linearize;
pub mod micro_qc;
pub mod wave;
```

Write `crates/consensus/src/bullshark/wave.rs`:

```rust
//! Wave indexing: a wave `w` covers rounds `4w..=4w+3` (whitepaper Ch. 8.1).

use types::primitives::{Round, Wave};

pub const ROUNDS_PER_WAVE: u64 = 4;

pub fn wave_of(round: Round) -> Wave {
    Wave::new(round.as_u64() / ROUNDS_PER_WAVE)
}

pub fn round_in_wave(round: Round) -> u64 {
    round.as_u64() % ROUNDS_PER_WAVE
}

pub fn first_round_of(wave: Wave) -> Round {
    Round::new(wave.as_u64() * ROUNDS_PER_WAVE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_indexing_is_consistent() {
        for r in 0..40 {
            let round = Round::new(r);
            let wave = wave_of(round);
            assert_eq!(wave.as_u64(), r / 4);
            assert_eq!(round_in_wave(round), r % 4);
        }
    }

    #[test]
    fn first_round_round_trips() {
        for w in 0..10 {
            assert_eq!(wave_of(first_round_of(Wave::new(w))).as_u64(), w);
        }
    }
}
```

Write `crates/consensus/src/bullshark/anchor.rs`:

```rust
//! Anchor selection per wave via stake-weighted ECVRF sortition (Ch. 8.1).
//!
//! Body deferred to L2 implementation plan.

use types::crypto_types::Hash32;
use types::primitives::{ValidatorId, Wave};

pub fn select_anchor(_wave: Wave, _candidates: &[(ValidatorId, Hash32)]) -> Option<ValidatorId> {
    None
}
```

Write `crates/consensus/src/bullshark/commit.rs`:

```rust
//! Wave commit rules — shortcut (2 rounds) + slow path (4 rounds) (Ch. 8.2).
//!
//! Body deferred to L2 implementation plan.

use types::crypto_types::Hash32;
use types::primitives::Wave;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitOutcome {
    Pending,
    Shortcut(Hash32),
    SlowPath(Hash32),
    Failed,
}

pub fn try_commit(_wave: Wave) -> CommitOutcome {
    CommitOutcome::Pending
}
```

Write `crates/consensus/src/bullshark/linearize.rs`:

```rust
//! `Closure(Aw)` BFS with vertex-hash tie-break (Ch. 8.3).
//!
//! Body deferred to L2 implementation plan.

use types::crypto_types::Hash32;

pub fn closure(_anchor: Hash32) -> Vec<Hash32> {
    Vec::new()
}
```

Write `crates/consensus/src/bullshark/micro_qc.rs`:

```rust
//! MicroQC aggregation when partials reach `⌈2/3 · C⌉` (Ch. 8.4).
//!
//! Body deferred to L2 implementation plan.

use types::micro::{MicroQc, MicroVote};

pub fn try_assemble(_votes: &[MicroVote]) -> Option<MicroQc> {
    None
}
```

- [ ] **Step 2: Wire**

Edit `crates/consensus/src/lib.rs` — add `pub mod bullshark;` to the module list (alphabetical insertion after `action`):

```rust
//! LUA-DAG consensus state machine — pure, deterministic.

pub mod action;
pub mod bullshark;
pub mod config;
pub mod event;
pub mod lock_macro;
pub mod ports;
pub mod prelude;
pub mod state_machine;

pub use action::{Action, BlobStatus};
pub use config::ConsensusConfig;
pub use event::Event;
pub use state_machine::StateMachine;
```

- [ ] **Step 3: Build + run wave tests**

Run: `cargo test -p consensus`
Expected: `bullshark::wave` unit tests pass.

- [ ] **Step 4: Commit**

```powershell
git add crates/consensus
git commit -m "feat(consensus): add bullshark module tree (wave indexing + L2 stubs)"
```

---

## Task 14: `consensus` crate — macro_fin module tree + AggregationMode

**Files:**
- Create: `crates/consensus/src/macro_fin/mod.rs`
- Create: `crates/consensus/src/macro_fin/window.rs`
- Create: `crates/consensus/src/macro_fin/proposer.rs`
- Create: `crates/consensus/src/macro_fin/checkpoint.rs`
- Create: `crates/consensus/src/macro_fin/aggregation/mod.rs`
- Create: `crates/consensus/src/macro_fin/aggregation/mode0_flat.rs`
- Create: `crates/consensus/src/macro_fin/aggregation/mode_a_subnet.rs`
- Create: `crates/consensus/src/macro_fin/aggregation/mode_b_leaderless.rs`
- Create: `crates/consensus/src/macro_fin/aggregation/subnet.rs`
- Create: `crates/consensus/src/macro_fin/macro_qc.rs`
- Create: `crates/consensus/src/macro_fin/two_chain.rs`
- Create: `crates/consensus/src/macro_fin/vote_book.rs`
- Modify: `crates/consensus/src/lib.rs`

- [ ] **Step 1: Aggregation mode selector**

Write `crates/consensus/src/macro_fin/aggregation/mod.rs`:

```rust
//! Adaptive BLS aggregation modes (whitepaper Eq. 9.1).

pub mod mode0_flat;
pub mod mode_a_subnet;
pub mod mode_b_leaderless;
pub mod subnet;

use crate::config::ConsensusConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregationMode {
    /// `Ne < SUBNET_FLAT_THRESHOLD` — all validators sign a single root.
    Mode0Flat,
    /// `SUBNET_FLAT_THRESHOLD ≤ Ne < SUBNET_FULL_THRESHOLD` — primary subnet plan.
    ModeASubnet,
    /// `Ne ≥ SUBNET_FULL_THRESHOLD` or subnet primary missed — leaderless fallback.
    ModeBLeaderless,
}

/// Decide aggregation mode from validator-set size `ne` and config thresholds.
pub fn select(ne: u32, config: &ConsensusConfig) -> AggregationMode {
    if ne < config.subnet_flat_threshold {
        AggregationMode::Mode0Flat
    } else if ne < config.subnet_full_threshold {
        AggregationMode::ModeASubnet
    } else {
        AggregationMode::ModeBLeaderless
    }
}

/// Choose `Ke` (number of subnets) for Mode A / Mode B (whitepaper Eq. 9.1).
/// Body deferred to macro-finality implementation plan.
pub fn select_ke(_ne: u32, _config: &ConsensusConfig) -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_selection_thresholds() {
        let c = ConsensusConfig::default();
        assert_eq!(select(100, &c), AggregationMode::Mode0Flat);
        assert_eq!(select(500, &c), AggregationMode::ModeASubnet);
        assert_eq!(select(999, &c), AggregationMode::ModeASubnet);
        assert_eq!(select(1000, &c), AggregationMode::ModeBLeaderless);
        assert_eq!(select(5000, &c), AggregationMode::ModeBLeaderless);
    }
}
```

Write `crates/consensus/src/macro_fin/aggregation/mode0_flat.rs`:

```rust
//! Mode 0: flat aggregation. `Ne < SUBNET_FLAT_THRESHOLD`.
//!
//! Body deferred to macro-finality implementation plan.
```

Write `crates/consensus/src/macro_fin/aggregation/mode_a_subnet.rs`:

```rust
//! Mode A: subnet-rotated aggregation per epoch.
//!
//! Body deferred to macro-finality implementation plan.
```

Write `crates/consensus/src/macro_fin/aggregation/mode_b_leaderless.rs`:

```rust
//! Mode B: leaderless fallback when subnet primary misses.
//!
//! Body deferred to macro-finality implementation plan.
```

Write `crates/consensus/src/macro_fin/aggregation/subnet.rs`:

```rust
//! Subnet assignment: `subnet(v_i, e) = H(pubkey ‖ R_macro) mod K_e` (Ch. 9.2).

use crypto::hash::{Domain, blake3_tagged};
use types::crypto_types::{BlsPubkey, Hash32};

pub fn assign(pubkey: &BlsPubkey, r_macro: Hash32, ke: u32) -> u32 {
    let mut payload = [0u8; 48 + 32];
    payload[..48].copy_from_slice(&pubkey.0);
    payload[48..].copy_from_slice(r_macro.as_bytes());
    let h = blake3_tagged(Domain::SubnetAssign, &payload);
    let first8 = u64::from_le_bytes(h.0[..8].try_into().expect("8 bytes"));
    (first8 % ke as u64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignment_is_in_range() {
        let pk = BlsPubkey([7u8; 48]);
        let r = Hash32([3u8; 32]);
        for ke in [1, 4, 16, 64u32] {
            let s = assign(&pk, r, ke);
            assert!(s < ke);
        }
    }

    #[test]
    fn assignment_is_deterministic() {
        let pk = BlsPubkey([7u8; 48]);
        let r = Hash32([3u8; 32]);
        assert_eq!(assign(&pk, r, 32), assign(&pk, r, 32));
    }
}
```

Add `crypto.workspace = true` to `[dependencies]` in `crates/consensus/Cargo.toml` if not already present (it is — from Task 9).

- [ ] **Step 2: Other macro_fin stubs**

Write `crates/consensus/src/macro_fin/mod.rs`:

```rust
//! L3 — Macro-finality (whitepaper Ch. 9).

pub mod aggregation;
pub mod checkpoint;
pub mod macro_qc;
pub mod proposer;
pub mod two_chain;
pub mod vote_book;
pub mod window;

pub use aggregation::{AggregationMode, select as select_aggregation_mode};
```

Write `crates/consensus/src/macro_fin/window.rs`:

```rust
//! Macro-window cadence: `W = 8` micro-slots (whitepaper Ch. 9.1).

use crate::config::ConsensusConfig;
use types::primitives::Round;

pub fn is_macro_boundary(round: Round, config: &ConsensusConfig) -> bool {
    round.as_u64() % (config.macro_window_w as u64) == 0
}
```

Write `crates/consensus/src/macro_fin/proposer.rs`:

```rust
//! Macro proposer selection: primary + backup, with `T_macropropose = 4s`.
//!
//! Body deferred to macro-finality implementation plan.

use types::primitives::{Height, ValidatorId};

pub fn primary_proposer(_height: Height) -> Option<ValidatorId> {
    None
}

pub fn backup_proposer(_height: Height) -> Option<ValidatorId> {
    None
}
```

Write `crates/consensus/src/macro_fin/checkpoint.rs`:

```rust
//! Build and verify `MacroCheckpoint` payloads.
//!
//! Body deferred to macro-finality implementation plan.
```

Write `crates/consensus/src/macro_fin/macro_qc.rs`:

```rust
//! `MacroQC` verification, signed-stake calculation, Mode B tie-break.
//!
//! Body deferred to macro-finality implementation plan.
```

Write `crates/consensus/src/macro_fin/two_chain.rs`:

```rust
//! Casper-FFG 2-chain finality rule (whitepaper Ch. 9.3).
//!
//! Body deferred to macro-finality implementation plan.
```

Write `crates/consensus/src/macro_fin/vote_book.rs`:

```rust
//! Per-validator vote history, epoch-indexed. Used by surround-vote detection
//! and lock_macro. Body deferred to macro-finality implementation plan.
```

- [ ] **Step 3: Wire**

Edit `crates/consensus/src/lib.rs` — insert `pub mod macro_fin;` alphabetically:

```rust
//! LUA-DAG consensus state machine — pure, deterministic.

pub mod action;
pub mod bullshark;
pub mod config;
pub mod event;
pub mod lock_macro;
pub mod macro_fin;
pub mod ports;
pub mod prelude;
pub mod state_machine;

pub use action::{Action, BlobStatus};
pub use config::ConsensusConfig;
pub use event::Event;
pub use state_machine::StateMachine;
```

- [ ] **Step 4: Verify**

Run: `cargo test -p consensus`
Expected: All tests pass (aggregation mode selector + subnet assign).

- [ ] **Step 5: Commit**

```powershell
git add crates/consensus
git commit -m "feat(consensus): add macro_fin tree with aggregation-mode selector and subnet assign"
```

---

## Task 15: `consensus` crate — leader, slashing, api trees

**Files:**
- Create: `crates/consensus/src/leader/mod.rs`
- Create: `crates/consensus/src/leader/beacon.rs`
- Create: `crates/consensus/src/leader/vrf_sortition.rs`
- Create: `crates/consensus/src/leader/reputation.rs`
- Create: `crates/consensus/src/leader/timeout.rs`
- Create: `crates/consensus/src/slashing/mod.rs`
- Create: `crates/consensus/src/slashing/evidence.rs`
- Create: `crates/consensus/src/slashing/equivocation.rs`
- Create: `crates/consensus/src/slashing/surround.rs`
- Create: `crates/consensus/src/slashing/inactivity_leak.rs`
- Create: `crates/consensus/src/slashing/penalty.rs`
- Create: `crates/consensus/src/api/mod.rs`
- Create: `crates/consensus/src/api/tier.rs`
- Create: `crates/consensus/src/api/query.rs`
- Modify: `crates/consensus/src/lib.rs`

- [ ] **Step 1: Leader tree**

Write `crates/consensus/src/leader/mod.rs`:

```rust
//! Leader election + per-wave timing primitives used by both L2 and L3.

pub mod beacon;
pub mod reputation;
pub mod timeout;
pub mod vrf_sortition;
```

Write `crates/consensus/src/leader/beacon.rs`:

```rust
//! Beacon chaining: `R_w = H(R_{w-1} ‖ MacroQC)` and the macro-height variant
//! `R_macro_h` (whitepaper Eq. 8.1).

use crypto::hash::{Domain, blake3_tagged};
use types::crypto_types::Hash32;

pub fn next_beacon(prev: Hash32, macro_qc_root: Hash32) -> Hash32 {
    let mut payload = [0u8; 64];
    payload[..32].copy_from_slice(prev.as_bytes());
    payload[32..].copy_from_slice(macro_qc_root.as_bytes());
    blake3_tagged(Domain::BeaconChain, &payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_beacon_is_deterministic_and_uses_domain_separation() {
        let prev = Hash32([1u8; 32]);
        let qc = Hash32([2u8; 32]);
        let a = next_beacon(prev, qc);
        let b = next_beacon(prev, qc);
        assert_eq!(a, b);
        // Same inputs into a different domain must differ.
        let other = blake3_tagged(Domain::SubnetAssign, &[1u8; 64]);
        assert_ne!(a, other);
    }
}
```

Write `crates/consensus/src/leader/vrf_sortition.rs`:

```rust
//! Stake-weighted ECVRF sortition: score `y_i · W / (w_i · rep_i)`
//! (whitepaper Ch. 8.1). Body deferred to leader-election plan.

use types::crypto_types::{Hash32, VrfProof};
use types::primitives::{StakeWeight, ValidatorId};

pub fn sortition_score(
    _y: Hash32,
    _total_weight: StakeWeight,
    _w_i: StakeWeight,
    _rep_i_micro: u64,
) -> u128 {
    0
}

pub fn validate_proof(_proof: &VrfProof, _alpha: &[u8], _validator: ValidatorId) -> bool {
    false
}
```

Write `crates/consensus/src/leader/reputation.rs`:

```rust
//! Shoal reputation tracker. Reputation is clamped to `[REP_MIN, REP_MAX]`
//! from [`crate::config::ConsensusConfig`] (default `[0.8, 1.2]`).
//!
//! Body deferred to leader-election plan.

use types::primitives::ValidatorId;

pub fn reputation_micro(_validator: ValidatorId) -> u64 {
    1_000_000
}
```

Write `crates/consensus/src/leader/timeout.rs`:

```rust
//! Centralized timer kinds used by both L2 (wave) and L3 (macro window).
//!
//! Body deferred to runtime-integration plan; `TimerId` payload values are
//! assigned here so action/event arms can match on them.

use types::primitives::TimerId;

pub const TIMER_KIND_BITS: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TimerKind {
    WaveDeadline = 0,
    MacroProposeDeadline = 1,
    SubnetDeadline = 2,
    CanonicalizeDeadline = 3,
}

pub fn encode(kind: TimerKind, seq: u64) -> TimerId {
    TimerId::new((seq << TIMER_KIND_BITS) | (kind as u64))
}

pub fn decode_kind(id: TimerId) -> Option<TimerKind> {
    match id.as_u64() & ((1u64 << TIMER_KIND_BITS) - 1) {
        0 => Some(TimerKind::WaveDeadline),
        1 => Some(TimerKind::MacroProposeDeadline),
        2 => Some(TimerKind::SubnetDeadline),
        3 => Some(TimerKind::CanonicalizeDeadline),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_kind_round_trip() {
        for &k in &[
            TimerKind::WaveDeadline,
            TimerKind::MacroProposeDeadline,
            TimerKind::SubnetDeadline,
            TimerKind::CanonicalizeDeadline,
        ] {
            let id = encode(k, 42);
            assert_eq!(decode_kind(id), Some(k));
        }
    }
}
```

- [ ] **Step 2: Slashing tree**

Write `crates/consensus/src/slashing/mod.rs`:

```rust
//! Slashing detectors and penalty computation (whitepaper Ch. 9.4 and Ch. 13).

pub mod equivocation;
pub mod evidence;
pub mod inactivity_leak;
pub mod penalty;
pub mod surround;
```

Write `crates/consensus/src/slashing/evidence.rs`:

```rust
//! Pure verifier for `SlashEvidence`. Body deferred to slashing plan.

use types::slashing::SlashEvidence;

pub fn verify(_evidence: &SlashEvidence) -> bool {
    false
}
```

Write `crates/consensus/src/slashing/equivocation.rs`:

```rust
//! Macro-equivocation detector (100% slash). Body deferred to slashing plan.
```

Write `crates/consensus/src/slashing/surround.rs`:

```rust
//! Casper-FFG surround-vote detector. Scans `vote_book`. Body deferred.
```

Write `crates/consensus/src/slashing/inactivity_leak.rs`:

```rust
//! Inactivity leak: 0.5% of stake per window after `inactivity_leak_threshold_windows`
//! consecutive unfinalized windows (whitepaper Ch. 9.4). Body deferred.

use crate::config::ConsensusConfig;
use types::primitives::StakeWeight;

pub fn per_window_leak(weight: StakeWeight, config: &ConsensusConfig) -> StakeWeight {
    let bp = config.inactivity_leak_bp_per_window as u128;
    let amount = weight.as_u128().saturating_mul(bp) / 10_000;
    StakeWeight::new(amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leak_is_50_basis_points_of_stake() {
        let c = ConsensusConfig::default();
        let s = StakeWeight::new(10_000);
        assert_eq!(per_window_leak(s, &c).as_u128(), 50); // 0.5%
    }
}
```

Write `crates/consensus/src/slashing/penalty.rs`:

```rust
//! Penalty arithmetic: double-vote 50%, DA failure 5%/incident, cap 50%
//! (whitepaper Ch. 13). Body deferred to slashing plan.
```

- [ ] **Step 3: API tree**

Write `crates/consensus/src/api/mod.rs`:

```rust
//! External query surface for downstream RPC servers (`node`, `cli`).

pub mod query;
pub mod tier;

pub use tier::BlobStatusTier;
```

Write `crates/consensus/src/api/tier.rs`:

```rust
//! Blob lifecycle tier exposed to clients (whitepaper Appendix A).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlobStatusTier {
    Accepted,
    SoftConfirmed,
    Justified,
    Finalized,
    EpochFinalized,
}

impl From<crate::action::BlobStatus> for BlobStatusTier {
    fn from(value: crate::action::BlobStatus) -> Self {
        match value {
            crate::action::BlobStatus::Accepted => Self::Accepted,
            crate::action::BlobStatus::SoftConfirmed => Self::SoftConfirmed,
            crate::action::BlobStatus::Justified => Self::Justified,
            crate::action::BlobStatus::Finalized => Self::Finalized,
            crate::action::BlobStatus::EpochFinalized => Self::EpochFinalized,
        }
    }
}
```

Write `crates/consensus/src/api/query.rs`:

```rust
//! Query interface backed by `StateMachine` + `Persistence`. Body deferred
//! to API-integration plan.

use types::crypto_types::Hash32;
use types::primitives::{BlobId, Height};

use crate::api::tier::BlobStatusTier;

pub trait ConsensusQuery {
    fn latest_finalized(&self) -> Option<Height>;
    fn micro_head(&self) -> Option<Hash32>;
    fn blob_status(&self, blob: BlobId) -> BlobStatusTier;
}
```

- [ ] **Step 4: Wire**

Replace `crates/consensus/src/lib.rs`:

```rust
//! LUA-DAG consensus state machine — pure, deterministic.

pub mod action;
pub mod api;
pub mod bullshark;
pub mod config;
pub mod event;
pub mod leader;
pub mod lock_macro;
pub mod macro_fin;
pub mod ports;
pub mod prelude;
pub mod slashing;
pub mod state_machine;

pub use action::{Action, BlobStatus};
pub use config::ConsensusConfig;
pub use event::Event;
pub use state_machine::StateMachine;
```

- [ ] **Step 5: Build + tests + clippy**

Run: `cargo test -p consensus && cargo clippy -p consensus -- -D warnings`
Expected: All tests pass (leader::beacon, leader::timeout, slashing::inactivity_leak); no warnings.

- [ ] **Step 6: Commit**

```powershell
git add crates/consensus
git commit -m "feat(consensus): add leader, slashing, and api module trees"
```

---

## Task 16: `net` crate skeleton (libp2p adapter)

**Files:**
- Create: `crates/net/Cargo.toml`
- Create: `crates/net/src/lib.rs`
- Create: `crates/net/src/config.rs`
- Create: `crates/net/src/transport.rs`
- Create: `crates/net/src/identity.rs`
- Create: `crates/net/src/error.rs`
- Create: `crates/net/src/gossip/mod.rs`
- Create: `crates/net/src/gossip/topics.rs`
- Create: `crates/net/src/gossip/codec.rs`
- Create: `crates/net/src/gossip/publisher.rs`
- Create: `crates/net/src/rpc/mod.rs`
- Create: `crates/net/src/rpc/causal_set.rs`
- Create: `crates/net/src/rpc/checkpoint_sync.rs`
- Create: `crates/net/src/peers/mod.rs`
- Create: `crates/net/src/peers/scoring.rs`
- Create: `crates/net/src/peers/discovery.rs`
- Create: `crates/net/src/bridge.rs`

- [ ] **Step 1: Crate manifest**

Write `crates/net/Cargo.toml`:

```toml
[package]
name = "net"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
types.workspace = true
consensus.workspace = true
libp2p.workspace = true
tokio.workspace = true
futures.workspace = true
borsh.workspace = true
serde.workspace = true
tracing.workspace = true
thiserror.workspace = true
```

- [ ] **Step 2: lib.rs**

Write `crates/net/src/lib.rs`:

```rust
//! libp2p (gossipsub + QUIC) adapter for LUA-DAG.
//!
//! Single seam to the outside world is `bridge::Bridge`, which translates
//! libp2p events into `consensus::Event` and consensus `Action`s into
//! libp2p publish/RPC calls.

pub mod bridge;
pub mod config;
pub mod error;
pub mod gossip;
pub mod identity;
pub mod peers;
pub mod rpc;
pub mod transport;
```

Write `crates/net/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetError {
    #[error("libp2p transport error: {0}")]
    Transport(String),

    #[error("gossip publish failed: {0}")]
    Publish(String),

    #[error("peer not found")]
    PeerNotFound,
}
```

Write `crates/net/src/config.rs`:

```rust
use std::net::SocketAddr;

#[derive(Clone, Debug)]
pub struct NetConfig {
    pub listen: Vec<SocketAddr>,
    pub bootstrap: Vec<String>,
    pub gossip_heartbeat_ms: u64,
    pub gossip_history_length: u32,
    pub gossip_history_gossip: u32,
}

impl Default for NetConfig {
    fn default() -> Self {
        Self {
            listen: vec![],
            bootstrap: vec![],
            gossip_heartbeat_ms: 700,
            gossip_history_length: 5,
            gossip_history_gossip: 3,
        }
    }
}
```

Write `crates/net/src/transport.rs`:

```rust
//! QUIC primary + TCP fallback with noise-xx and yamux. Body deferred to
//! transport-integration plan.
```

Write `crates/net/src/identity.rs`:

```rust
//! libp2p `PeerId` ↔ `ValidatorId` mapping; rotates per epoch. Body deferred.
```

- [ ] **Step 3: Gossip module — Topic enum is real**

Write `crates/net/src/gossip/mod.rs`:

```rust
pub mod codec;
pub mod publisher;
pub mod topics;

pub use topics::Topic;
```

Write `crates/net/src/gossip/topics.rs`:

```rust
/// Gossipsub topics broadcast across the validator mesh.
///
/// Topic strings are the **canonical wire identifier** — never reorder or
/// rename without a wire-version bump.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Topic {
    CertifiedVertex,
    MicroQc,
    MacroProposal,
    BlsPartial { subnet: u32 },
    SubnetAggregate,
    SlashEvidence,
}

impl Topic {
    pub fn as_topic_string(&self) -> String {
        match self {
            Topic::CertifiedVertex => "/luadag/1/vertex".into(),
            Topic::MicroQc => "/luadag/1/micro-qc".into(),
            Topic::MacroProposal => "/luadag/1/macro-proposal".into(),
            Topic::BlsPartial { subnet } => format!("/luadag/1/bls-partial/{subnet}"),
            Topic::SubnetAggregate => "/luadag/1/subnet-aggregate".into(),
            Topic::SlashEvidence => "/luadag/1/slash".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_strings_are_stable() {
        assert_eq!(Topic::CertifiedVertex.as_topic_string(), "/luadag/1/vertex");
        assert_eq!(
            Topic::BlsPartial { subnet: 7 }.as_topic_string(),
            "/luadag/1/bls-partial/7"
        );
    }
}
```

Write `crates/net/src/gossip/codec.rs`:

```rust
//! Borsh encode/decode for each topic payload. Body deferred to gossip plan.
```

Write `crates/net/src/gossip/publisher.rs`:

```rust
//! Publish + de-duplication ring for outbound messages. Body deferred.
```

- [ ] **Step 4: RPC + peers stubs**

Write `crates/net/src/rpc/mod.rs`:

```rust
pub mod causal_set;
pub mod checkpoint_sync;
```

Write `crates/net/src/rpc/causal_set.rs`:

```rust
//! Causal-set RPC for L1 sync. Placeholder until availability DAG crate lands.
```

Write `crates/net/src/rpc/checkpoint_sync.rs`:

```rust
//! Fast-sync for late-joining validators (snapshot + recent macros). Body deferred.
```

Write `crates/net/src/peers/mod.rs`:

```rust
pub mod discovery;
pub mod scoring;
```

Write `crates/net/src/peers/scoring.rs`:

```rust
//! Gossipsub score + ban heuristics. Body deferred to peer-management plan.
```

Write `crates/net/src/peers/discovery.rs`:

```rust
//! Kademlia DHT (optional) + bootstrap peer list. Body deferred.
```

- [ ] **Step 5: Bridge skeleton**

Write `crates/net/src/bridge.rs`:

```rust
//! The single adapter between libp2p and the consensus state machine.
//!
//! Consensus never imports libp2p directly. `Bridge` owns the libp2p swarm
//! and translates:
//! - inbound libp2p events  → [`consensus::Event`]
//! - outbound [`consensus::Action`] → libp2p publish / RPC
//!
//! Body deferred to network-integration plan.

use consensus::{Action, Event};

use crate::error::NetError;

pub struct Bridge;

impl Bridge {
    pub fn ingest(&mut self, _libp2p_event: ()) -> Vec<Event> {
        Vec::new()
    }

    pub fn dispatch(&mut self, _action: Action) -> Result<(), NetError> {
        Ok(())
    }
}
```

- [ ] **Step 6: Build + clippy**

Run: `cargo build -p net && cargo clippy -p net -- -D warnings`
Expected: Builds clean; no warnings.

- [ ] **Step 7: Commit**

```powershell
git add crates/net
git commit -m "feat(net): scaffold libp2p adapter with gossip topics and consensus bridge"
```

---

## Task 17: `storage` crate skeleton (RocksDB adapter)

**Files:**
- Create: `crates/storage/Cargo.toml`
- Create: `crates/storage/src/lib.rs`
- Create: `crates/storage/src/config.rs`
- Create: `crates/storage/src/error.rs`
- Create: `crates/storage/src/db.rs`
- Create: `crates/storage/src/columns.rs`
- Create: `crates/storage/src/keys.rs`
- Create: `crates/storage/src/wal.rs`
- Create: `crates/storage/src/gc.rs`
- Create: `crates/storage/src/snapshot.rs`
- Create: `crates/storage/src/persistence_impl.rs`
- Create: `crates/storage/src/stores/mod.rs`
- Create: `crates/storage/src/stores/vertex_store.rs`
- Create: `crates/storage/src/stores/micro_store.rs`
- Create: `crates/storage/src/stores/macro_store.rs`
- Create: `crates/storage/src/stores/valset_store.rs`
- Create: `crates/storage/src/stores/slash_store.rs`
- Create: `crates/storage/src/stores/vote_book_store.rs`

- [ ] **Step 1: Crate manifest**

Write `crates/storage/Cargo.toml`:

```toml
[package]
name = "storage"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
types.workspace = true
consensus.workspace = true
rocksdb.workspace = true
borsh.workspace = true
thiserror.workspace = true
tracing.workspace = true
```

- [ ] **Step 2: lib.rs and error**

Write `crates/storage/src/lib.rs`:

```rust
//! RocksDB-backed durable storage for LUA-DAG.
//!
//! Implements the `consensus::ports::Persistence` trait. Logical column
//! families are declared in [`columns`]; per-domain accessors live in
//! [`stores`].

pub mod columns;
pub mod config;
pub mod db;
pub mod error;
pub mod gc;
pub mod keys;
pub mod persistence_impl;
pub mod snapshot;
pub mod stores;
pub mod wal;

pub use error::StorageError;
pub use persistence_impl::PersistenceStore;
```

Write `crates/storage/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("rocksdb: {0}")]
    RocksDb(#[from] rocksdb::Error),

    #[error("decode: {0}")]
    Decode(String),

    #[error("not found")]
    NotFound,
}
```

Write `crates/storage/src/config.rs`:

```rust
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct StorageConfig {
    pub path: PathBuf,
    pub create_if_missing: bool,
    pub disable_wal: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("data/db"),
            create_if_missing: true,
            disable_wal: false,
        }
    }
}
```

- [ ] **Step 3: Column families**

Write `crates/storage/src/columns.rs`:

```rust
//! Logical column families. Key encoding for each is documented inline.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnFamily {
    /// `Vertex` indexed by `(round_be_u64, author_be32)` → CertifiedVertex.
    Vertex,
    /// `Micro` indexed by `(slot_be_u64)` → MicroCheckpoint + MicroQC.
    Micro,
    /// `Macro` indexed by `(height_be_u64)` → MacroCheckpoint + MacroQC.
    Macro,
    /// `Valset` indexed by `(epoch_be_u64)` → ValidatorSetSnapshot.
    Valset,
    /// `Slash` indexed by `(append_seq_be_u64)` → SlashEvidence; append-only.
    Slash,
    /// `VoteBook` indexed by `(validator_be32, epoch_be_u64)` → vote history.
    VoteBook,
    /// `Meta` for chain-tip pointers, version, etc.
    Meta,
}

impl ColumnFamily {
    pub const ALL: &'static [ColumnFamily] = &[
        ColumnFamily::Vertex,
        ColumnFamily::Micro,
        ColumnFamily::Macro,
        ColumnFamily::Valset,
        ColumnFamily::Slash,
        ColumnFamily::VoteBook,
        ColumnFamily::Meta,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            ColumnFamily::Vertex => "vertex",
            ColumnFamily::Micro => "micro",
            ColumnFamily::Macro => "macro",
            ColumnFamily::Valset => "valset",
            ColumnFamily::Slash => "slash",
            ColumnFamily::VoteBook => "vote_book",
            ColumnFamily::Meta => "meta",
        }
    }
}
```

- [ ] **Step 4: db.rs (column-family bootstrap)**

Write `crates/storage/src/db.rs`:

```rust
use rocksdb::{ColumnFamilyDescriptor, DB, Options};

use crate::columns::ColumnFamily;
use crate::config::StorageConfig;
use crate::error::StorageError;

pub struct Database {
    pub(crate) db: DB,
}

impl Database {
    pub fn open(config: &StorageConfig) -> Result<Self, StorageError> {
        let mut opts = Options::default();
        opts.create_if_missing(config.create_if_missing);
        opts.create_missing_column_families(true);

        let cfs: Vec<_> = ColumnFamily::ALL
            .iter()
            .map(|cf| ColumnFamilyDescriptor::new(cf.name(), Options::default()))
            .collect();

        let db = DB::open_cf_descriptors(&opts, &config.path, cfs)?;
        Ok(Self { db })
    }
}
```

- [ ] **Step 5: Keys**

Write `crates/storage/src/keys.rs`:

```rust
//! Big-endian numeric key encoding so RocksDB iterator scans in ascending
//! protocol order (rounds, heights, epochs).

pub fn u64_be(v: u64) -> [u8; 8] {
    v.to_be_bytes()
}

pub fn vertex_key(round: u64, author: &[u8; 32]) -> [u8; 40] {
    let mut out = [0u8; 40];
    out[..8].copy_from_slice(&u64_be(round));
    out[8..].copy_from_slice(author);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_sort_lexicographically_by_round() {
        let a = vertex_key(1, &[0u8; 32]);
        let b = vertex_key(2, &[0u8; 32]);
        assert!(a < b);
    }
}
```

- [ ] **Step 6: wal, gc, snapshot stubs**

Write `crates/storage/src/wal.rs`:

```rust
//! Write-ahead log batch helper for atomic multi-CF writes. Body deferred
//! to durability plan.
```

Write `crates/storage/src/gc.rs`:

```rust
//! Three-tier pruning: hot (200 rounds), warm (10k rounds), cold (long-term).
//! Body deferred to GC plan; horizons read from `ConsensusConfig`.
```

Write `crates/storage/src/snapshot.rs`:

```rust
//! State snapshot for fast-sync and weak-subjectivity bootstrap. Body deferred.
```

- [ ] **Step 7: stores subtree (per-domain accessors)**

Write `crates/storage/src/stores/mod.rs`:

```rust
pub mod macro_store;
pub mod micro_store;
pub mod slash_store;
pub mod valset_store;
pub mod vertex_store;
pub mod vote_book_store;
```

Write each `crates/storage/src/stores/<name>_store.rs` with a one-line doc and an empty impl module:

```rust
// vertex_store.rs
//! `CertifiedVertex` accessor by `(round, author)`. Body deferred to L1 plan.
```

```rust
// micro_store.rs
//! `MicroCheckpoint` + `MicroQC` accessor by slot. Body deferred to L2 plan.
```

```rust
// macro_store.rs
//! `MacroCheckpoint` + `MacroQC` + 2-chain pointers. Body deferred to L3 plan.
```

```rust
// valset_store.rs
//! `ValidatorSetSnapshot` accessor per epoch. Body deferred to epoch plan.
```

```rust
// slash_store.rs
//! Append-only slash evidence log. Body deferred to slashing plan.
```

```rust
// vote_book_store.rs
//! Per-validator vote history for surround-vote detection. Body deferred.
```

- [ ] **Step 8: Persistence trait impl skeleton**

Write `crates/storage/src/persistence_impl.rs`:

```rust
//! Implementation of [`consensus::ports::Persistence`] over RocksDB.
//! Body deferred to durability plan; the impl methods return `NotFound`
//! / `Ok(())` so the skeleton compiles.

use consensus::ports::{Persistence, PersistenceError};
use types::macros::qc::MacroQc;
use types::micro::MicroQc;
use types::primitives::Height;
use types::slashing::SlashEvidence;

use crate::db::Database;

pub struct PersistenceStore {
    #[allow(dead_code)]
    pub(crate) db: Database,
}

impl PersistenceStore {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

impl Persistence for PersistenceStore {
    fn put_macro_qc(&mut self, _qc: &MacroQc) -> Result<(), PersistenceError> {
        Ok(())
    }

    fn get_macro_qc(&self, _height: Height) -> Result<Option<MacroQc>, PersistenceError> {
        Ok(None)
    }

    fn put_micro_qc(&mut self, _qc: &MicroQc) -> Result<(), PersistenceError> {
        Ok(())
    }

    fn append_slash_evidence(&mut self, _evidence: &SlashEvidence) -> Result<(), PersistenceError> {
        Ok(())
    }
}
```

- [ ] **Step 9: Build + tests**

Run: `cargo test -p storage`
Expected: keys test passes; crate builds.

> ⚠ The `rocksdb` crate requires a C++ toolchain (cmake + clang/libclang). If the build fails with linker errors, install the platform-specific toolchain — see `rocksdb` crate docs. This is a one-time environment setup, not a plan defect.

- [ ] **Step 10: Commit**

```powershell
git add crates/storage
git commit -m "feat(storage): scaffold rocksdb adapter with column families and Persistence impl skeleton"
```

---

## Task 18: `apps/node` skeleton

**Files:**
- Create: `apps/node/Cargo.toml`
- Create: `apps/node/src/main.rs`
- Create: `apps/node/src/args.rs`
- Create: `apps/node/src/config.rs`
- Create: `apps/node/src/runtime.rs`
- Create: `apps/node/src/orchestrator.rs`
- Create: `apps/node/src/timer.rs`
- Create: `apps/node/src/validator_set_loader.rs`
- Create: `apps/node/src/rpc_server.rs`
- Create: `apps/node/src/shutdown.rs`
- Create: `apps/node/src/observability/mod.rs`
- Create: `apps/node/src/observability/metrics.rs`
- Create: `apps/node/src/observability/tracing.rs`
- Create: `apps/node/src/observability/health.rs`

- [ ] **Step 1: Crate manifest**

Write `apps/node/Cargo.toml`:

```toml
[package]
name = "node"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[[bin]]
name = "node"
path = "src/main.rs"

[dependencies]
types.workspace = true
crypto.workspace = true
consensus.workspace = true
net.workspace = true
storage.workspace = true

tokio.workspace = true
clap.workspace = true
toml.workspace = true
serde.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
prometheus.workspace = true
anyhow.workspace = true
```

- [ ] **Step 2: main.rs + args + config**

Write `apps/node/src/main.rs`:

```rust
mod args;
mod config;
mod observability;
mod orchestrator;
mod rpc_server;
mod runtime;
mod shutdown;
mod timer;
mod validator_set_loader;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let args = args::CliArgs::parse();
    observability::tracing::init();
    tracing::info!(config = ?args.config, "starting lua-dag node");

    let rt = runtime::build()?;
    rt.block_on(async {
        let _orchestrator = orchestrator::Orchestrator::new();
        // Run-loop body deferred to node-integration plan.
        shutdown::wait_for_signal().await;
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}
```

Write `apps/node/src/args.rs`:

```rust
use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "node", about = "LUA-DAG validator node")]
pub struct CliArgs {
    /// Path to the node TOML config (default: `config/node.toml`).
    #[arg(long, default_value = "config/node.toml")]
    pub config: PathBuf,
}
```

Write `apps/node/src/config.rs`:

```rust
use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct NodeConfig {
    pub data_dir: String,
    pub listen: Vec<String>,
    pub bootstrap: Vec<String>,
    pub rpc_listen: String,
    pub metrics_listen: String,
}

pub fn load(path: &Path) -> Result<NodeConfig> {
    let text = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&text)?)
}
```

- [ ] **Step 3: runtime + orchestrator + timer**

Write `apps/node/src/runtime.rs`:

```rust
use anyhow::Result;
use tokio::runtime::{Builder, Runtime};

pub fn build() -> Result<Runtime> {
    Ok(Builder::new_multi_thread().enable_all().build()?)
}
```

Write `apps/node/src/orchestrator.rs`:

```rust
//! Glue between `StateMachine`, `net::Bridge`, `storage::PersistenceStore`,
//! and `timer`. Body deferred to node-integration plan.

use consensus::{ConsensusConfig, StateMachine};

pub struct Orchestrator {
    #[allow(dead_code)]
    state_machine: StateMachine,
}

impl Orchestrator {
    pub fn new() -> Self {
        Self {
            state_machine: StateMachine::new(ConsensusConfig::default()),
        }
    }
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
    }
}
```

Write `apps/node/src/timer.rs`:

```rust
//! `Clock` impl using `std::time::Instant`. Schedules timers and emits
//! `Event::TimerFired`. Body deferred to node-integration plan.

use std::time::Instant;

use consensus::ports::Clock;

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}
```

Write `apps/node/src/validator_set_loader.rs`:

```rust
//! Bootstrap validator set + epoch transition. Body deferred.
```

- [ ] **Step 4: observability tree**

Write `apps/node/src/observability/mod.rs`:

```rust
pub mod health;
pub mod metrics;
pub mod tracing;
```

Write `apps/node/src/observability/tracing.rs`:

```rust
use tracing_subscriber::{EnvFilter, fmt};

pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).with_target(false).init();
}
```

Write `apps/node/src/observability/metrics.rs`:

```rust
//! Prometheus exporter. Body deferred to observability plan.
```

Write `apps/node/src/observability/health.rs`:

```rust
//! Readiness + liveness HTTP endpoints. Body deferred.
```

- [ ] **Step 5: rpc + shutdown**

Write `apps/node/src/rpc_server.rs`:

```rust
//! External JSON-RPC server (see whitepaper §5.3 and spec §12). Body deferred.
```

Write `apps/node/src/shutdown.rs`:

```rust
use tokio::signal;

pub async fn wait_for_signal() {
    let _ = signal::ctrl_c().await;
}
```

- [ ] **Step 6: Build**

Run: `cargo build -p node`
Expected: Builds clean.

- [ ] **Step 7: Commit**

```powershell
git add apps/node
git commit -m "feat(node): scaffold validator binary (tokio runtime, orchestrator, observability)"
```

---

## Task 19: `apps/sim` skeleton (deterministic simulator)

**Files:**
- Create: `apps/sim/Cargo.toml`
- Create: `apps/sim/src/main.rs`
- Create: `apps/sim/src/args.rs`
- Create: `apps/sim/src/world.rs`
- Create: `apps/sim/src/virtual_clock.rs`
- Create: `apps/sim/src/virtual_net.rs`
- Create: `apps/sim/src/virtual_dag.rs`
- Create: `apps/sim/src/virtual_beacon.rs`
- Create: `apps/sim/src/replay.rs`
- Create: `apps/sim/src/metrics.rs`
- Create: `apps/sim/src/adversary/mod.rs`
- Create: `apps/sim/src/adversary/byzantine.rs`
- Create: `apps/sim/src/adversary/network.rs`
- Create: `apps/sim/src/scenarios/mod.rs`
- Create: `apps/sim/src/scenarios/happy_path.rs`
- Create: `apps/sim/src/scenarios/anchor_dos.rs`
- Create: `apps/sim/src/scenarios/mode_b_fallback.rs`
- Create: `apps/sim/src/scenarios/equivocation_inject.rs`
- Create: `apps/sim/src/scenarios/byzantine_split.rs`
- Create: `apps/sim/src/scenarios/network_partition.rs`
- Create: `apps/sim/src/checker/mod.rs`
- Create: `apps/sim/src/checker/safety.rs`
- Create: `apps/sim/src/checker/liveness.rs`
- Create: `apps/sim/src/checker/lock_macro.rs`

- [ ] **Step 1: Crate manifest (no net, no storage)**

Write `apps/sim/Cargo.toml`:

```toml
[package]
name = "sim"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[[bin]]
name = "sim"
path = "src/main.rs"

[dependencies]
types.workspace = true
crypto.workspace = true
consensus.workspace = true

clap.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
anyhow.workspace = true
```

- [ ] **Step 2: main.rs + args**

Write `apps/sim/src/main.rs`:

```rust
mod adversary;
mod args;
mod checker;
mod metrics;
mod replay;
mod scenarios;
mod virtual_beacon;
mod virtual_clock;
mod virtual_dag;
mod virtual_net;
mod world;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let args = args::CliArgs::parse();
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    tracing::info!(?args, "starting simulator");
    let _world = world::World::new(args.validators, args.seed);
    // Run loop deferred to simulator implementation plan.
    Ok(())
}
```

Write `apps/sim/src/args.rs`:

```rust
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "sim", about = "LUA-DAG deterministic adversarial simulator")]
pub struct CliArgs {
    #[arg(long, default_value_t = 4)]
    pub validators: u32,

    #[arg(long, default_value_t = 100)]
    pub rounds: u32,

    #[arg(long, default_value_t = 0)]
    pub seed: u64,

    /// Scenario name (e.g. `happy_path`, `anchor_dos`, `mode_b_fallback`).
    #[arg(long, default_value = "happy_path")]
    pub scenario: String,
}
```

- [ ] **Step 3: world, virtual_*, adversary**

Write `apps/sim/src/world.rs`:

```rust
use consensus::{ConsensusConfig, StateMachine};

pub struct World {
    #[allow(dead_code)]
    machines: Vec<StateMachine>,
    #[allow(dead_code)]
    seed: u64,
}

impl World {
    pub fn new(validators: u32, seed: u64) -> Self {
        let cfg = ConsensusConfig::default();
        let machines = (0..validators).map(|_| StateMachine::new(cfg.clone())).collect();
        Self { machines, seed }
    }
}
```

Write `apps/sim/src/virtual_clock.rs`:

```rust
use std::time::{Duration, Instant};

use consensus::ports::Clock;

pub struct VirtualClock {
    base: Instant,
    offset: Duration,
}

impl VirtualClock {
    pub fn new() -> Self {
        Self { base: Instant::now(), offset: Duration::ZERO }
    }

    pub fn advance(&mut self, by: Duration) {
        self.offset += by;
    }
}

impl Default for VirtualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for VirtualClock {
    fn now(&self) -> Instant {
        self.base + self.offset
    }
}
```

Write `apps/sim/src/virtual_net.rs`:

```rust
//! In-memory message bus with adversary-controlled scheduling. Body deferred.
```

Write `apps/sim/src/virtual_dag.rs`:

```rust
//! In-memory `DagView` impl (L1 placeholder). Body deferred.

use std::collections::HashMap;

use consensus::ports::DagView;
use types::crypto_types::Hash32;
use types::dag::CertifiedVertex;
use types::primitives::Round;

#[derive(Default)]
pub struct VirtualDag {
    vertices: HashMap<Hash32, CertifiedVertex>,
}

impl DagView for VirtualDag {
    fn certified_vertex(&self, hash: Hash32) -> Option<CertifiedVertex> {
        self.vertices.get(&hash).cloned()
    }

    fn parents(&self, hash: Hash32) -> Vec<Hash32> {
        self.vertices
            .get(&hash)
            .map(|v| v.vertex.parents.clone())
            .unwrap_or_default()
    }

    fn round_vertices(&self, round: Round) -> Vec<Hash32> {
        self.vertices
            .iter()
            .filter(|(_, v)| v.vertex.round == round)
            .map(|(h, _)| *h)
            .collect()
    }
}
```

Write `apps/sim/src/virtual_beacon.rs`:

```rust
//! Deterministic in-memory `RandomnessBeacon`. Body deferred.
```

Write `apps/sim/src/replay.rs`:

```rust
//! `--seed S` reproduces a run bit-identically. Body deferred.
```

Write `apps/sim/src/metrics.rs`:

```rust
//! Finality-latency histograms (p50 / p95). Body deferred.
```

Write `apps/sim/src/adversary/mod.rs`:

```rust
pub mod byzantine;
pub mod network;
```

Write `apps/sim/src/adversary/byzantine.rs`:

```rust
//! Byzantine adversaries: equivocate, withhold, surround. Body deferred.
```

Write `apps/sim/src/adversary/network.rs`:

```rust
//! Network adversaries: drop/delay/duplicate; partition. Body deferred.
```

- [ ] **Step 4: scenarios + checker stubs**

Write `apps/sim/src/scenarios/mod.rs`:

```rust
pub mod anchor_dos;
pub mod byzantine_split;
pub mod equivocation_inject;
pub mod happy_path;
pub mod mode_b_fallback;
pub mod network_partition;
```

For each scenario file, write a one-line doc comment placeholder:

```rust
// happy_path.rs
//! All validators online, no adversary. Body deferred to simulator plan.
```

```rust
// anchor_dos.rs
//! 1/3 of stake offline; commit must still progress via slow path. Body deferred.
```

```rust
// mode_b_fallback.rs
//! Macro-proposer missed back-to-back; aggregation must fall back to Mode B. Body deferred.
```

```rust
// equivocation_inject.rs
//! Inject macro-equivocation evidence; slash detector must fire. Body deferred.
```

```rust
// byzantine_split.rs
//! Up to f Byzantine validators voting both sides of a partition. Body deferred.
```

```rust
// network_partition.rs
//! Time-bounded partition; liveness must recover. Body deferred.
```

Write `apps/sim/src/checker/mod.rs`:

```rust
pub mod liveness;
pub mod lock_macro;
pub mod safety;
```

```rust
// safety.rs
//! Invariant: no two finalized macros conflict. Body deferred.
```

```rust
// liveness.rs
//! Invariant: finality progresses under healthy stake. Body deferred.
```

```rust
// lock_macro.rs
//! Invariant: §13.5 cross-layer lock_macro. Body deferred.
```

- [ ] **Step 5: Build**

Run: `cargo build -p sim`
Expected: Builds clean.

- [ ] **Step 6: Commit**

```powershell
git add apps/sim
git commit -m "feat(sim): scaffold deterministic adversarial simulator (no net, no storage)"
```

---

## Task 20: `apps/cli` skeleton

**Files:**
- Create: `apps/cli/Cargo.toml`
- Create: `apps/cli/src/main.rs`
- Create: `apps/cli/src/args.rs`
- Create: `apps/cli/src/commands/mod.rs`
- Create: `apps/cli/src/commands/inspect.rs`
- Create: `apps/cli/src/commands/keygen.rs`
- Create: `apps/cli/src/commands/verify.rs`
- Create: `apps/cli/src/commands/replay_log.rs`
- Create: `apps/cli/src/commands/bench_aggregate.rs`

- [ ] **Step 1: Crate manifest**

Write `apps/cli/Cargo.toml`:

```toml
[package]
name = "cli"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[[bin]]
name = "luadag"
path = "src/main.rs"

[dependencies]
types.workspace = true
crypto.workspace = true
storage.workspace = true

clap.workspace = true
anyhow.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
```

- [ ] **Step 2: main + args + commands tree**

Write `apps/cli/src/main.rs`:

```rust
mod args;
mod commands;

use anyhow::Result;
use args::{CliArgs, Command};
use clap::Parser;

fn main() -> Result<()> {
    let args = CliArgs::parse();
    match args.command {
        Command::Inspect(c) => commands::inspect::run(c),
        Command::Keygen(c) => commands::keygen::run(c),
        Command::Verify(c) => commands::verify::run(c),
        Command::ReplayLog(c) => commands::replay_log::run(c),
        Command::BenchAggregate(c) => commands::bench_aggregate::run(c),
    }
}
```

Write `apps/cli/src/args.rs`:

```rust
use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "luadag", about = "LUA-DAG dev / inspect / ops tool")]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Inspect(Inspect),
    Keygen(Keygen),
    Verify(Verify),
    ReplayLog(ReplayLog),
    BenchAggregate(BenchAggregate),
}

#[derive(Parser, Debug)]
pub struct Inspect {
    #[arg(long)]
    pub db: PathBuf,
    #[arg(long)]
    pub height: Option<u64>,
}

#[derive(Parser, Debug)]
pub struct Keygen {
    #[arg(long)]
    pub out_dir: PathBuf,
}

#[derive(Parser, Debug)]
pub struct Verify {
    #[arg(long)]
    pub evidence: PathBuf,
}

#[derive(Parser, Debug)]
pub struct ReplayLog {
    #[arg(long)]
    pub log: PathBuf,
}

#[derive(Parser, Debug)]
pub struct BenchAggregate {
    #[arg(long, default_value_t = 1000)]
    pub partials: u32,
}
```

Write `apps/cli/src/commands/mod.rs`:

```rust
pub mod bench_aggregate;
pub mod inspect;
pub mod keygen;
pub mod replay_log;
pub mod verify;
```

For each command file, write a stub that compiles and prints what it would do:

```rust
// inspect.rs
use anyhow::Result;

use crate::args::Inspect;

pub fn run(args: Inspect) -> Result<()> {
    println!("inspect db={:?} height={:?}", args.db, args.height);
    Ok(())
}
```

```rust
// keygen.rs
use anyhow::Result;

use crate::args::Keygen;

pub fn run(args: Keygen) -> Result<()> {
    println!("keygen out_dir={:?}", args.out_dir);
    Ok(())
}
```

```rust
// verify.rs
use anyhow::Result;

use crate::args::Verify;

pub fn run(args: Verify) -> Result<()> {
    println!("verify evidence={:?}", args.evidence);
    Ok(())
}
```

```rust
// replay_log.rs
use anyhow::Result;

use crate::args::ReplayLog;

pub fn run(args: ReplayLog) -> Result<()> {
    println!("replay log={:?}", args.log);
    Ok(())
}
```

```rust
// bench_aggregate.rs
use anyhow::Result;

use crate::args::BenchAggregate;

pub fn run(args: BenchAggregate) -> Result<()> {
    println!("bench_aggregate partials={}", args.partials);
    Ok(())
}
```

- [ ] **Step 3: Build + smoke test the binary**

Run: `cargo build -p cli && cargo run -p cli -- keygen --out-dir tmp`
Expected: Prints `keygen out_dir="tmp"`.

- [ ] **Step 4: Commit**

```powershell
git add apps/cli
git commit -m "feat(cli): scaffold luadag dev/inspect/ops binary with clap subcommands"
```

---

## Task 21: Workspace-level directories + license files

**Files:**
- Create: `LICENSE-APACHE`
- Create: `LICENSE-MIT`
- Create: `tests/.gitkeep`
- Create: `tests/common/mod.rs`
- Create: `benches/.gitkeep`
- Create: `fuzz/.gitkeep`
- Create: `fuzz/README.md`
- Create: `config/node.example.toml`
- Create: `scripts/run-devnet.ps1`
- Create: `scripts/run-devnet.sh`
- Create: `docs/architecture/.gitkeep`

- [ ] **Step 0: License files**

Write `LICENSE-APACHE` with the full text of the Apache License 2.0 (verbatim from https://www.apache.org/licenses/LICENSE-2.0.txt — copy the standard text including the appendix).

Write `LICENSE-MIT` with the standard MIT license text:

```
MIT License

Copyright (c) 2026 LUA-DAG Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

- [ ] **Step 1: Reserved workspace dirs**

Write `tests/.gitkeep`: (empty file)

Write `tests/common/mod.rs`:

```rust
//! Shared fixtures for workspace-level integration tests. Body grows as
//! cross-crate integration tests are added in later plans.
```

Write `benches/.gitkeep`: (empty file)

Write `fuzz/.gitkeep`: (empty file)

Write `fuzz/README.md`:

```markdown
# Fuzz targets

`cargo-fuzz` requires the nightly toolchain. Targets in this directory are
deliberately not part of the main workspace `Cargo.toml` so the workspace
stays on stable.

Add new targets with `cargo +nightly fuzz add <name>` from this directory
once it is initialized via `cargo +nightly fuzz init`.
```

- [ ] **Step 2: Sample config (whitepaper Table 17.1 defaults in TOML)**

Write `config/node.example.toml`:

```toml
# LUA-DAG node configuration — defaults match whitepaper Table 17.1.

data_dir = "data/db"
listen = ["/ip4/0.0.0.0/udp/30303/quic-v1"]
bootstrap = []
rpc_listen = "127.0.0.1:8545"
metrics_listen = "127.0.0.1:9090"

[consensus]
round_duration_ms = 250
macro_window_w = 8
micro_committee_size = 256
t_macropropose_ms = 4000
t_subnet_ms = 2000
t_canonicalize_ms = 8000
subnet_flat_threshold = 500
subnet_full_threshold = 1000
btc_confirmations_for_final = 6
reputation_min_micro = 800000
reputation_max_micro = 1200000
slash_equivocation_bp = 10000
slash_double_vote_bp = 5000
inactivity_leak_bp_per_window = 50
inactivity_leak_threshold_windows = 4
gc_hot_horizon_rounds = 200
gc_warm_horizon_rounds = 10000
```

- [ ] **Step 3: Dev scripts**

Write `scripts/run-devnet.ps1`:

```powershell
# Run a 4-validator local devnet. Stub — body filled in by devnet plan.
Write-Host "TODO: spin up 4 local node processes with shared bootstrap"
exit 1
```

Write `scripts/run-devnet.sh`:

```bash
#!/usr/bin/env bash
# Run a 4-validator local devnet. Stub — body filled in by devnet plan.
echo "TODO: spin up 4 local node processes with shared bootstrap"
exit 1
```

Write `docs/architecture/.gitkeep`: (empty file)

- [ ] **Step 4: Commit**

```powershell
git add LICENSE-APACHE LICENSE-MIT tests benches fuzz config scripts docs/architecture
git commit -m "chore: add license files and scaffold workspace dirs (tests, benches, fuzz, config, scripts)"
```

---

## Task 22: CI workflows + final workspace verification

**Files:**
- Create: `.github/workflows/ci.yml`
- Create: `.github/workflows/audit.yml`

- [ ] **Step 1: CI workflow**

Write `.github/workflows/ci.yml`:

```yaml
name: ci

on:
  push:
    branches: [main]
  pull_request:

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-D warnings"

jobs:
  fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all --check

  clippy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --workspace --all-targets -- -D warnings

  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace --all-targets

  build-windows:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --workspace

  deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
```

- [ ] **Step 2: Audit workflow (scheduled)**

Write `.github/workflows/audit.yml`:

```yaml
name: audit

on:
  schedule:
    - cron: "0 6 * * 1"  # weekly Monday 06:00 UTC
  workflow_dispatch:

jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
```

- [ ] **Step 3: Final workspace verification**

Run, in order:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo build --workspace
```

Expected: All four pass. Test count should include:
- `types`: primitives_basics (4 tests), codec_roundtrip (6 tests), bls bitmap (1 test).
- `crypto`: hash_domain_separation (2 tests), bls bitmap (covered above).
- `consensus`: config_defaults (2 tests), bullshark::wave (2 tests), macro_fin::aggregation (2 tests), macro_fin::aggregation::subnet (2 tests), leader::beacon (1 test), leader::timeout (1 test), slashing::inactivity_leak (1 test).
- `storage`: keys (1 test).

If any step fails:
- `fmt` failure → run `cargo fmt --all` and re-stage.
- `clippy` failure → fix or `#[allow(...)]` with a one-line justification.
- `rocksdb` link error on Windows → install Visual Studio Build Tools (C++ workload) + LLVM; re-run.

- [ ] **Step 4: Commit**

```powershell
git add .github
git commit -m "ci: add fmt, clippy, test, windows build, and cargo-deny workflows"
```

- [ ] **Step 5: Push and open PR (optional, ask user first)**

```powershell
git status
git log --oneline -25
```

Ask the user whether to push the branch and open a PR — do not push automatically.

---

## Closing notes

After Task 22, the repository has the complete architecture from the design spec:

- All 5 library crates and 3 binary crates compile under stable Rust 2024.
- Cross-crate contracts (`ConsensusConfig`, `Event`, `Action`, 5 port traits, `BlobStatus`, `AggregationMode`, `Topic`, `ColumnFamily`, `SlashEvidence`) are real types — downstream code can be written against them now.
- Algorithm bodies (Bullshark wave/anchor/commit/linearize, macro-finality aggregation modes A/B, slashing detectors, libp2p wire formats, RocksDB schema details) remain stubs; each is the subject of its own subsequent implementation plan.
- CI enforces fmt, clippy, tests, Windows build, and license/advisory scanning on every PR.

Subsequent plans should pick up by crate or by feature (e.g., `2026-05-13-bullshark-wave-commit.md`, `2026-05-14-macro-finality-aggregation.md`, `2026-05-15-storage-rocksdb-schema.md`) and import these contracts unchanged.
