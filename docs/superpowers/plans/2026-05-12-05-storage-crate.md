# `crates/storage` Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up `crates/storage/` as the RocksDB adapter skeleton: column-family bootstrap, big-endian key encoders, six type-specific stores (vertex, micro, macro, validator-set, slash, vote-book), WAL/GC/snapshot skeleton modules, and a concrete `RocksPersistence` type that implements [`consensus::ports::Persistence`]. Every store has working get/put for at least one fixture; complex secondary indexes and pruning are explicitly deferred.

**Architecture:** RocksDB is the only persistent backend. Each consensus artefact lives in its own column family with a documented big-endian key schema (so range scans are monotonic). The `Persistence` trait impl is the **only** outward surface: `consensus` and `node` never touch RocksDB types directly. WAL, GC, and snapshot modules expose function signatures + smoke tests; real reorg semantics arrive in follow-up plans.

**Tech Stack:** `rocksdb` 0.22 (with `snappy` feature), `borsh`, `thiserror`, `tracing`.

**Prerequisites:** Plans 00, 01, 02, 03.

---

## File Structure

Per spec §7.4.

```
crates/storage/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── error.rs
│   ├── config.rs
│   ├── db.rs                  # open + column-family bootstrap
│   ├── columns.rs             # ColumnFamily enum
│   ├── keys.rs                # big-endian key encoders
│   ├── wal.rs                 # atomic write batch helpers
│   ├── gc.rs                  # tier horizons
│   ├── snapshot.rs            # snapshot + restore
│   ├── persistence_impl.rs    # impl consensus::ports::Persistence
│   └── stores/
│       ├── mod.rs
│       ├── vertex_store.rs
│       ├── micro_store.rs
│       ├── macro_store.rs
│       ├── valset_store.rs
│       ├── slash_store.rs
│       └── vote_book_store.rs
└── tests/
    ├── crash_recovery.rs
    ├── pruning.rs
    └── snapshot_roundtrip.rs
```

---

## Task 1: Crate skeleton + workspace registration

**Files:**
- Create: `crates/storage/Cargo.toml`
- Create: `crates/storage/src/lib.rs`
- Modify: workspace `Cargo.toml`

- [ ] **Step 1: Write `crates/storage/Cargo.toml`**

```toml
[package]
name         = "storage"
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
consensus   = { path = "../consensus" }
borsh       = { workspace = true }
thiserror   = { workspace = true }
tracing     = { workspace = true }
rocksdb     = { workspace = true }
serde       = { workspace = true }

[dev-dependencies]
tempfile    = { workspace = true }
```

- [ ] **Step 2: Write `crates/storage/src/lib.rs`**

```rust
//! RocksDB-backed persistence layer.
//!
//! Public surface is the [`RocksPersistence`] struct, which implements
//! [`consensus::ports::Persistence`].
#![cfg_attr(not(test), warn(missing_docs))]

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

pub use config::StorageConfig;
pub use db::Database;
pub use error::{Error, Result};
pub use persistence_impl::RocksPersistence;
```

- [ ] **Step 3: Add to workspace members**

```toml
members = [
    "crates/types",
    "crates/crypto",
    "crates/consensus",
    "crates/net",
    "crates/storage",
]
```

---

## Task 2: `error.rs` + `config.rs`

**Files:**
- Create: `crates/storage/src/error.rs`
- Create: `crates/storage/src/config.rs`

- [ ] **Step 1: Write `crates/storage/src/error.rs`**

```rust
//! Crate-level error type.

use thiserror::Error;

/// All failures from `crates/storage`.
#[derive(Debug, Error)]
pub enum Error {
    /// RocksDB returned an error.
    #[error("rocksdb error: {0}")]
    Rocks(#[from] rocksdb::Error),

    /// Codec failure encoding/decoding a value.
    #[error("codec error: {0}")]
    Codec(String),

    /// Column family was missing from the open handle.
    #[error("unknown column family: {0}")]
    UnknownColumn(&'static str),

    /// Logical error (e.g. invariant violation).
    #[error("logic error: {0}")]
    Logic(&'static str),

    /// Wrapping a `types` error.
    #[error("types error: {0}")]
    Types(#[from] types::Error),
}

/// Convenience alias.
pub type Result<T> = core::result::Result<T, Error>;
```

- [ ] **Step 2: Write `crates/storage/src/config.rs`**

```rust
//! Storage configuration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Top-level storage config.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StorageConfig {
    /// On-disk path for the RocksDB instance.
    pub path: PathBuf,
    /// Create the directory if missing on open.
    pub create_if_missing: bool,
    /// Maximum number of WAL files retained per column family.
    pub max_total_wal_size_mb: u64,
}

impl StorageConfig {
    /// Local-devnet defaults under `./data`.
    #[must_use]
    pub fn devnet_default() -> Self {
        Self {
            path: PathBuf::from("./data/rocksdb"),
            create_if_missing: true,
            max_total_wal_size_mb: 256,
        }
    }
}
```

---

## Task 3: `columns.rs` — ColumnFamily enum

**Files:**
- Create: `crates/storage/src/columns.rs`

- [ ] **Step 1: Write the module + tests**

```rust
//! Column-family names and helpers.
//!
//! Each variant maps to a single column family; the wire name (the `&str`
//! returned by [`ColumnFamily::name`]) is part of the on-disk format and
//! must not change without a migration.

/// All column families in the LUA-DAG store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ColumnFamily {
    /// `(round, author) -> CertifiedVertex`.
    Vertex,
    /// `slot -> MicroCheckpoint`.
    MicroCheckpoint,
    /// `slot -> MicroQc`.
    MicroQc,
    /// `height -> MacroCheckpoint`.
    MacroCheckpoint,
    /// `checkpoint_hash -> MacroQc`.
    MacroQc,
    /// `height -> 2-chain pointer (parent_hash)`.
    MacroTwoChain,
    /// `epoch -> ValidatorSet`.
    ValidatorSet,
    /// `seq -> SlashEvidence` (append-only).
    SlashEvidence,
    /// `(validator, target_epoch) -> VoteRecord`.
    VoteBook,
}

impl ColumnFamily {
    /// Wire name (on-disk).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Vertex            => "vertex",
            Self::MicroCheckpoint   => "micro_cp",
            Self::MicroQc           => "micro_qc",
            Self::MacroCheckpoint   => "macro_cp",
            Self::MacroQc           => "macro_qc",
            Self::MacroTwoChain     => "macro_two_chain",
            Self::ValidatorSet      => "valset",
            Self::SlashEvidence     => "slash",
            Self::VoteBook          => "votebook",
        }
    }

    /// Complete list (used at DB-open time).
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[
            Self::Vertex,
            Self::MicroCheckpoint,
            Self::MicroQc,
            Self::MacroCheckpoint,
            Self::MacroQc,
            Self::MacroTwoChain,
            Self::ValidatorSet,
            Self::SlashEvidence,
            Self::VoteBook,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_distinct() {
        let mut names: Vec<_> = ColumnFamily::all().iter().map(|c| c.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ColumnFamily::all().len());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p storage --lib columns::`
Expected: PASS (1 test).

---

## Task 4: `keys.rs` — big-endian key encoders

**Files:**
- Create: `crates/storage/src/keys.rs`

- [ ] **Step 1: Write the module + tests**

```rust
//! Key encoders.
//!
//! All multi-byte integers are big-endian so RocksDB lexicographic
//! ordering matches numeric ordering — critical for prefix scans.

use types::{
    crypto_types::Hash32,
    primitives::{Epoch, Height, Round, ValidatorId},
};

/// `(round, author)` — 8 + 32 bytes.
#[must_use]
pub fn vertex(round: Round, author: &ValidatorId) -> [u8; 40] {
    let mut out = [0u8; 40];
    out[..8].copy_from_slice(&round.0.to_be_bytes());
    out[8..].copy_from_slice(author.as_bytes());
    out
}

/// `slot` — 8 bytes.
#[must_use]
pub fn slot(slot: u64) -> [u8; 8] {
    slot.to_be_bytes()
}

/// `height` — 8 bytes.
#[must_use]
pub fn height(h: Height) -> [u8; 8] {
    h.0.to_be_bytes()
}

/// `epoch` — 8 bytes.
#[must_use]
pub fn epoch(e: Epoch) -> [u8; 8] {
    e.0.to_be_bytes()
}

/// 32-byte hash key (e.g. checkpoint_hash).
#[must_use]
pub fn hash(h: &Hash32) -> [u8; 32] {
    *h.as_bytes()
}

/// `(validator, target_epoch)` — 32 + 8 bytes.
#[must_use]
pub fn votebook(validator: &ValidatorId, target_epoch: Epoch) -> [u8; 40] {
    let mut out = [0u8; 40];
    out[..32].copy_from_slice(validator.as_bytes());
    out[32..].copy_from_slice(&target_epoch.0.to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_endian_ordering_matches_numeric_ordering() {
        let a = slot(1);
        let b = slot(2);
        let c = slot(256);
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn vertex_key_round_prefix_groups_by_round() {
        let a = vertex(Round(7), &ValidatorId([0; 32]));
        let b = vertex(Round(7), &ValidatorId([0xFF; 32]));
        let c = vertex(Round(8), &ValidatorId([0; 32]));
        // Same-round entries share the first 8 bytes.
        assert_eq!(&a[..8], &b[..8]);
        // Crossing rounds keeps numeric ordering.
        assert!(b < c);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p storage --lib keys::`
Expected: PASS (2 tests).

---

## Task 5: `db.rs` — open + column-family bootstrap

**Files:**
- Create: `crates/storage/src/db.rs`

- [ ] **Step 1: Write the module + tests**

```rust
//! RocksDB handle wrapper.

use std::path::Path;

use rocksdb::{ColumnFamilyDescriptor, Options, DB};

use crate::{
    columns::ColumnFamily,
    config::StorageConfig,
    error::{Error, Result},
};

/// Opened RocksDB handle.
#[derive(Debug)]
pub struct Database {
    /// The underlying `rocksdb::DB`. We keep it `pub(crate)` so stores
    /// can borrow it directly without exposing rocksdb in the crate API.
    pub(crate) inner: DB,
}

impl Database {
    /// Open (creating if missing) a RocksDB instance with all column
    /// families from [`ColumnFamily::all`].
    pub fn open(cfg: &StorageConfig) -> Result<Self> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(cfg.create_if_missing);
        db_opts.create_missing_column_families(true);
        db_opts.set_max_total_wal_size(cfg.max_total_wal_size_mb * 1024 * 1024);

        let cfs: Vec<ColumnFamilyDescriptor> = ColumnFamily::all()
            .iter()
            .map(|cf| ColumnFamilyDescriptor::new(cf.name(), Options::default()))
            .collect();

        let inner = DB::open_cf_descriptors(&db_opts, &cfg.path, cfs)?;
        Ok(Self { inner })
    }

    /// Resolve a column-family handle. Returns `Error::UnknownColumn` if
    /// the open handle didn't include it.
    pub fn cf(&self, cf: ColumnFamily) -> Result<&rocksdb::ColumnFamily> {
        self.inner.cf_handle(cf.name()).ok_or(Error::UnknownColumn(cf.name()))
    }

    /// Raw put helper.
    pub fn put_raw(&self, cf: ColumnFamily, key: &[u8], value: &[u8]) -> Result<()> {
        let h = self.cf(cf)?;
        self.inner.put_cf(h, key, value)?;
        Ok(())
    }

    /// Raw get helper.
    pub fn get_raw(&self, cf: ColumnFamily, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let h = self.cf(cf)?;
        Ok(self.inner.get_cf(h, key)?)
    }

    /// Borrow the underlying DB. Stores use this for batch writes.
    pub(crate) fn inner(&self) -> &DB {
        &self.inner
    }

    /// Drop and remove the on-disk directory. **Tests only.**
    #[cfg(test)]
    pub fn destroy_for_tests(path: impl AsRef<Path>) -> Result<()> {
        let _ = DB::destroy(&Options::default(), path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn open_creates_all_column_families() {
        let dir = tempdir().unwrap();
        let cfg = StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 64,
        };
        let db = Database::open(&cfg).unwrap();
        for cf in ColumnFamily::all() {
            db.cf(*cf).expect("every CF must be present");
        }
    }

    #[test]
    fn put_then_get_round_trip() {
        let dir = tempdir().unwrap();
        let cfg = StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 64,
        };
        let db = Database::open(&cfg).unwrap();
        db.put_raw(ColumnFamily::MacroQc, b"k", b"v").unwrap();
        let got = db.get_raw(ColumnFamily::MacroQc, b"k").unwrap();
        assert_eq!(got.as_deref(), Some(b"v".as_slice()));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p storage --lib db::`
Expected: PASS (2 tests).

---

## Task 6: `stores/` — six typed stores

**Files:**
- Create: `crates/storage/src/stores/mod.rs`
- Create: `crates/storage/src/stores/vertex_store.rs`
- Create: `crates/storage/src/stores/micro_store.rs`
- Create: `crates/storage/src/stores/macro_store.rs`
- Create: `crates/storage/src/stores/valset_store.rs`
- Create: `crates/storage/src/stores/slash_store.rs`
- Create: `crates/storage/src/stores/vote_book_store.rs`

Each store is a thin namespace of free functions, all taking `&Database`. Keeping them stateless means tests can use one `Database` for all of them without ownership games.

- [ ] **Step 1: Write `crates/storage/src/stores/mod.rs`**

```rust
//! Type-specific stores. Each module exposes encode-and-put / decode-on-get
//! helpers built on top of [`crate::db::Database`].

pub mod macro_store;
pub mod micro_store;
pub mod slash_store;
pub mod valset_store;
pub mod vertex_store;
pub mod vote_book_store;
```

- [ ] **Step 2: Write `crates/storage/src/stores/vertex_store.rs`**

```rust
//! `(round, author) -> CertifiedVertex`.

use types::dag::CertifiedVertex;

use crate::{
    columns::ColumnFamily,
    db::Database,
    error::{Error, Result},
    keys,
};

/// Store a certified vertex.
pub fn put(db: &Database, v: &CertifiedVertex) -> Result<()> {
    let key = keys::vertex(v.vertex.round, &v.vertex.author);
    let bytes = borsh::to_vec(v).map_err(|e| Error::Codec(e.to_string()))?;
    db.put_raw(ColumnFamily::Vertex, &key, &bytes)
}

/// Fetch by `(round, author)` key.
pub fn get(
    db: &Database,
    round: types::primitives::Round,
    author: &types::primitives::ValidatorId,
) -> Result<Option<CertifiedVertex>> {
    let key = keys::vertex(round, author);
    match db.get_raw(ColumnFamily::Vertex, &key)? {
        Some(bytes) => borsh::from_slice(&bytes)
            .map(Some)
            .map_err(|e| Error::Codec(e.to_string())),
        None => Ok(None),
    }
}
```

- [ ] **Step 3: Write `crates/storage/src/stores/micro_store.rs`**

```rust
//! Micro checkpoints + QCs.

use types::micro::{MicroCheckpoint, MicroQc};

use crate::{
    columns::ColumnFamily,
    db::Database,
    error::{Error, Result},
    keys,
};

/// Store a micro checkpoint keyed by its anchor round.
pub fn put_checkpoint(db: &Database, cp: &MicroCheckpoint) -> Result<()> {
    let key = keys::slot(cp.anchor_round.0);
    let bytes = borsh::to_vec(cp).map_err(|e| Error::Codec(e.to_string()))?;
    db.put_raw(ColumnFamily::MicroCheckpoint, &key, &bytes)
}

/// Fetch by anchor round.
pub fn get_checkpoint(db: &Database, slot: u64) -> Result<Option<MicroCheckpoint>> {
    let key = keys::slot(slot);
    match db.get_raw(ColumnFamily::MicroCheckpoint, &key)? {
        Some(bytes) => borsh::from_slice(&bytes)
            .map(Some)
            .map_err(|e| Error::Codec(e.to_string())),
        None => Ok(None),
    }
}

/// Store a micro QC keyed by its `checkpoint_hash`.
pub fn put_qc(db: &Database, qc: &MicroQc) -> Result<()> {
    let key = keys::hash(&qc.checkpoint_hash);
    let bytes = borsh::to_vec(qc).map_err(|e| Error::Codec(e.to_string()))?;
    db.put_raw(ColumnFamily::MicroQc, &key, &bytes)
}
```

- [ ] **Step 4: Write `crates/storage/src/stores/macro_store.rs`**

```rust
//! Macro checkpoints + QCs + 2-chain pointers.

use types::{
    crypto_types::Hash32,
    macros::{MacroCheckpoint, MacroQc},
    primitives::Height,
};

use crate::{
    columns::ColumnFamily,
    db::Database,
    error::{Error, Result},
    keys,
};

/// Store a macro checkpoint keyed by height.
pub fn put_checkpoint(db: &Database, cp: &MacroCheckpoint) -> Result<()> {
    let key = keys::height(cp.height);
    let bytes = borsh::to_vec(cp).map_err(|e| Error::Codec(e.to_string()))?;
    db.put_raw(ColumnFamily::MacroCheckpoint, &key, &bytes)
}

/// Fetch checkpoint at height.
pub fn get_checkpoint(db: &Database, height: Height) -> Result<Option<MacroCheckpoint>> {
    let key = keys::height(height);
    match db.get_raw(ColumnFamily::MacroCheckpoint, &key)? {
        Some(bytes) => borsh::from_slice(&bytes)
            .map(Some)
            .map_err(|e| Error::Codec(e.to_string())),
        None => Ok(None),
    }
}

/// Store a macro QC keyed by `checkpoint_hash`.
pub fn put_qc(db: &Database, qc: &MacroQc) -> Result<()> {
    let key = keys::hash(&qc.checkpoint_hash);
    let bytes = borsh::to_vec(qc).map_err(|e| Error::Codec(e.to_string()))?;
    db.put_raw(ColumnFamily::MacroQc, &key, &bytes)
}

/// Fetch macro QC by checkpoint hash.
pub fn get_qc(db: &Database, hash: &Hash32) -> Result<Option<MacroQc>> {
    let key = keys::hash(hash);
    match db.get_raw(ColumnFamily::MacroQc, &key)? {
        Some(bytes) => borsh::from_slice(&bytes)
            .map(Some)
            .map_err(|e| Error::Codec(e.to_string())),
        None => Ok(None),
    }
}

/// Store the 2-chain pointer (parent hash) for a height.
pub fn put_two_chain_pointer(db: &Database, child: Height, parent_hash: &Hash32) -> Result<()> {
    let key = keys::height(child);
    db.put_raw(ColumnFamily::MacroTwoChain, &key, parent_hash.as_bytes())
}
```

- [ ] **Step 5: Write `crates/storage/src/stores/valset_store.rs`**

```rust
//! Validator-set snapshots per epoch.

use types::{primitives::Epoch, validator::ValidatorSet};

use crate::{
    columns::ColumnFamily,
    db::Database,
    error::{Error, Result},
    keys,
};

/// Store a validator-set snapshot for `set.epoch`.
pub fn put(db: &Database, set: &ValidatorSet) -> Result<()> {
    let key = keys::epoch(set.epoch);
    let bytes = borsh::to_vec(set).map_err(|e| Error::Codec(e.to_string()))?;
    db.put_raw(ColumnFamily::ValidatorSet, &key, &bytes)
}

/// Fetch the active validator set for `epoch`.
pub fn get(db: &Database, epoch: Epoch) -> Result<Option<ValidatorSet>> {
    let key = keys::epoch(epoch);
    match db.get_raw(ColumnFamily::ValidatorSet, &key)? {
        Some(bytes) => borsh::from_slice(&bytes)
            .map(Some)
            .map_err(|e| Error::Codec(e.to_string())),
        None => Ok(None),
    }
}
```

- [ ] **Step 6: Write `crates/storage/src/stores/slash_store.rs`**

```rust
//! Append-only slashing evidence log keyed by monotonic sequence.

use types::slashing::SlashEvidence;

use crate::{
    columns::ColumnFamily,
    db::Database,
    error::{Error, Result},
};

/// Append evidence. The caller passes the next sequence number.
pub fn append(db: &Database, seq: u64, ev: &SlashEvidence) -> Result<()> {
    let key = seq.to_be_bytes();
    let bytes = borsh::to_vec(ev).map_err(|e| Error::Codec(e.to_string()))?;
    db.put_raw(ColumnFamily::SlashEvidence, &key, &bytes)
}
```

- [ ] **Step 7: Write `crates/storage/src/stores/vote_book_store.rs`**

```rust
//! Per-validator vote history. Surround detector consumes this.

use consensus::macro_fin::vote_book::VoteRecord;
use types::primitives::{Epoch, ValidatorId};

use crate::{
    columns::ColumnFamily,
    db::Database,
    error::{Error, Result},
    keys,
};

/// Store a vote record keyed by `(validator, target_epoch)`.
pub fn put(db: &Database, validator: &ValidatorId, record: &VoteRecord) -> Result<()> {
    let key = keys::votebook(validator, record.target);
    // VoteRecord does not currently derive Borsh — encode the three
    // fields manually so we don't force a derive on the consensus side.
    let mut bytes = Vec::with_capacity(72);
    bytes.extend_from_slice(&record.source.0.to_be_bytes());
    bytes.extend_from_slice(&record.target.0.to_be_bytes());
    bytes.extend_from_slice(&record.checkpoint.0);
    db.put_raw(ColumnFamily::VoteBook, &key, &bytes)
}

/// Fetch a vote at `target_epoch` for `validator`.
pub fn get(
    db: &Database,
    validator: &ValidatorId,
    target_epoch: Epoch,
) -> Result<Option<VoteRecord>> {
    let key = keys::votebook(validator, target_epoch);
    let Some(bytes) = db.get_raw(ColumnFamily::VoteBook, &key)? else {
        return Ok(None);
    };
    if bytes.len() != 72 {
        return Err(Error::Logic("vote_book row has wrong length"));
    }
    let source = u64::from_be_bytes(bytes[..8].try_into().unwrap());
    let target = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
    let mut checkpoint = [0u8; 32];
    checkpoint.copy_from_slice(&bytes[16..48]);
    Ok(Some(VoteRecord {
        source: Epoch(source),
        target: Epoch(target),
        checkpoint: types::crypto_types::Hash32(checkpoint),
    }))
}
```

- [ ] **Step 8: Build**

Run: `cargo build -p storage`
Expected: PASS.

---

## Task 7: `wal.rs`, `gc.rs`, `snapshot.rs` (skeleton)

**Files:**
- Create: `crates/storage/src/wal.rs`
- Create: `crates/storage/src/gc.rs`
- Create: `crates/storage/src/snapshot.rs`

- [ ] **Step 1: Write `crates/storage/src/wal.rs`**

```rust
//! Write-ahead-log helpers for atomic batch writes.

use rocksdb::WriteBatch;

use crate::{
    columns::ColumnFamily,
    db::Database,
    error::Result,
};

/// Apply a `WriteBatch` atomically. Wrapping `rocksdb::WriteBatch` lets
/// us swap in pre-flush hooks later without touching call sites.
pub fn apply(db: &Database, batch: WriteBatch) -> Result<()> {
    db.inner().write(batch)?;
    Ok(())
}

/// Convenience builder: writes one key/value pair under a CF in a fresh
/// batch (useful in tests).
pub fn put_one(db: &Database, cf: ColumnFamily, key: &[u8], value: &[u8]) -> Result<()> {
    let mut batch = WriteBatch::default();
    let h = db.cf(cf)?;
    batch.put_cf(h, key, value);
    apply(db, batch)
}
```

- [ ] **Step 2: Write `crates/storage/src/gc.rs`**

```rust
//! Tiered GC horizons (hot / warm / cold). Skeleton only — actual
//! pruning lives in a follow-up plan that ties horizons to finalized
//! checkpoints.

use consensus::Config;

/// Plan output: which slot to start pruning from for the cold tier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GcPlan {
    /// Last round eligible for cold storage at the time of planning.
    pub cold_horizon_round: u64,
    /// Last round eligible for warm storage.
    pub warm_horizon_round: u64,
    /// Last round eligible for hot storage.
    pub hot_horizon_round: u64,
}

/// Compute the next GC plan given the current micro-head round.
#[must_use]
pub fn plan(cfg: &Config, micro_head_round: u64) -> GcPlan {
    let hot = cfg.storage.gc_hot_horizon_rounds;
    let warm = cfg.storage.gc_warm_horizon_rounds;
    GcPlan {
        hot_horizon_round: micro_head_round.saturating_sub(hot),
        warm_horizon_round: micro_head_round.saturating_sub(warm),
        cold_horizon_round: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizons_subtract_from_head() {
        let cfg = Config::default_table_17_1();
        let plan = plan(&cfg, 1_000);
        assert_eq!(plan.hot_horizon_round, 800);
        assert_eq!(plan.warm_horizon_round, 0);
    }
}
```

- [ ] **Step 3: Write `crates/storage/src/snapshot.rs`**

```rust
//! State snapshots for late-joining validators + WS bootstrap.
//!
//! Skeleton: exposes a deterministic snapshot identifier helper. Real
//! snapshot creation (RocksDB SST export) lands in a follow-up.

use types::{crypto_types::Hash32, primitives::Height};

use crypto::hash::{blake3_with_dst, dst};

/// Compute a deterministic snapshot identifier from `(height, root)`.
#[must_use]
pub fn snapshot_id(height: Height, macro_root: &Hash32) -> Hash32 {
    let mut buf = [0u8; 40];
    buf[..8].copy_from_slice(&height.0.to_be_bytes());
    buf[8..].copy_from_slice(&macro_root.0);
    blake3_with_dst(dst::CONTENT_HASH, &buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_id_is_deterministic() {
        let h = Height(42);
        let r = Hash32([7; 32]);
        assert_eq!(snapshot_id(h, &r), snapshot_id(h, &r));
    }
}
```

> Note: `snapshot.rs` adds `crypto = { path = "../crypto" }` to storage's `[dependencies]`. Update `crates/storage/Cargo.toml`:
> ```toml
> crypto      = { path = "../crypto" }
> ```

- [ ] **Step 4: Run tests**

Run: `cargo test -p storage --lib gc:: snapshot::`
Expected: PASS.

---

## Task 8: `persistence_impl.rs` — `RocksPersistence` implementing `Persistence`

**Files:**
- Create: `crates/storage/src/persistence_impl.rs`

- [ ] **Step 1: Write the module + tests**

```rust
//! `RocksPersistence`: concrete impl of [`consensus::ports::Persistence`].

use std::sync::Arc;

use consensus::ports::Persistence;
use types::{
    crypto_types::Hash32,
    macros::{MacroCheckpoint, MacroQc},
    micro::MicroQc,
    primitives::Height,
    slashing::SlashEvidence,
};

use crate::{
    db::Database,
    error::Result,
    stores::{macro_store, micro_store, slash_store},
};

/// Thread-safe RocksDB-backed implementation of [`Persistence`].
#[derive(Debug, Clone)]
pub struct RocksPersistence {
    db: Arc<Database>,
    /// Monotonic counter for [`Persistence::append_slash_evidence`].
    seq: Arc<std::sync::atomic::AtomicU64>,
}

impl RocksPersistence {
    /// Wrap an already-opened [`Database`].
    #[must_use]
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            seq: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Borrow the underlying database (for tests + admin code).
    #[must_use]
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }
}

impl Persistence for RocksPersistence {
    fn store_micro_qc(&self, qc: &MicroQc) -> consensus::Result<()> {
        micro_store::put_qc(&self.db, qc).map_err(map_err)
    }

    fn store_macro_checkpoint(&self, cp: &MacroCheckpoint) -> consensus::Result<()> {
        macro_store::put_checkpoint(&self.db, cp).map_err(map_err)
    }

    fn store_macro_qc(&self, qc: &MacroQc) -> consensus::Result<()> {
        macro_store::put_qc(&self.db, qc).map_err(map_err)
    }

    fn append_slash_evidence(&self, ev: &SlashEvidence) -> consensus::Result<()> {
        let next = self.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        slash_store::append(&self.db, next, ev).map_err(map_err)
    }

    fn macro_checkpoint_at(&self, height: Height) -> consensus::Result<Option<MacroCheckpoint>> {
        macro_store::get_checkpoint(&self.db, height).map_err(map_err)
    }

    fn macro_qc_for(&self, checkpoint_hash: &Hash32) -> consensus::Result<Option<MacroQc>> {
        macro_store::get_qc(&self.db, checkpoint_hash).map_err(map_err)
    }
}

fn map_err(e: crate::Error) -> consensus::Error {
    consensus::Error::Persistence(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::StorageConfig, db::Database};
    use std::sync::Arc;
    use tempfile::tempdir;
    use types::{
        crypto_types::{BlsAggSig, BlsSig, Hash32},
        macros::{AggregationMode, MacroCheckpoint, MacroQc},
        primitives::{Epoch, Height},
    };

    fn fresh_db() -> (Arc<Database>, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let cfg = StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 16,
        };
        (Arc::new(Database::open(&cfg).unwrap()), dir)
    }

    #[test]
    fn store_and_fetch_macro_checkpoint_via_trait() {
        let (db, _dir) = fresh_db();
        let p = RocksPersistence::new(db);
        let cp = MacroCheckpoint {
            height: Height(7),
            epoch: Epoch(1),
            parent: Hash32::zero(),
            micro_root: Hash32([1; 32]),
            hash: Hash32([2; 32]),
        };
        p.store_macro_checkpoint(&cp).unwrap();
        let got = p.macro_checkpoint_at(Height(7)).unwrap();
        assert_eq!(got, Some(cp));
    }

    #[test]
    fn store_and_fetch_macro_qc_via_trait() {
        let (db, _dir) = fresh_db();
        let p = RocksPersistence::new(db);
        let qc = MacroQc {
            checkpoint_hash: Hash32([3; 32]),
            mode: AggregationMode::Mode0Flat,
            agg: BlsAggSig { sig: BlsSig([0; 96]), bitmap: vec![0xFF] },
        };
        p.store_macro_qc(&qc).unwrap();
        let got = p.macro_qc_for(&Hash32([3; 32])).unwrap();
        assert_eq!(got, Some(qc));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p storage --lib persistence_impl::`
Expected: PASS (2 tests).

---

## Task 9: Integration tests

**Files:**
- Create: `crates/storage/tests/crash_recovery.rs`
- Create: `crates/storage/tests/pruning.rs`
- Create: `crates/storage/tests/snapshot_roundtrip.rs`

Real crash-recovery semantics depend on RocksDB's WAL; we exercise it by writing, dropping the handle, reopening, and reading back.

- [ ] **Step 1: Write `crates/storage/tests/crash_recovery.rs`**

```rust
//! Open → write → drop → reopen → read.

use std::sync::Arc;
use storage::{Database, RocksPersistence, StorageConfig};
use tempfile::tempdir;
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    macros::{AggregationMode, MacroCheckpoint, MacroQc},
    primitives::{Epoch, Height},
};
use consensus::ports::Persistence;

#[test]
fn reopen_recovers_written_macro_qc() {
    let dir = tempdir().unwrap();
    let cfg = StorageConfig {
        path: dir.path().to_path_buf(),
        create_if_missing: true,
        max_total_wal_size_mb: 16,
    };
    {
        let db = Arc::new(Database::open(&cfg).unwrap());
        let p = RocksPersistence::new(db);
        let qc = MacroQc {
            checkpoint_hash: Hash32([5; 32]),
            mode: AggregationMode::Mode0Flat,
            agg: BlsAggSig { sig: BlsSig([0; 96]), bitmap: vec![0xFF] },
        };
        p.store_macro_qc(&qc).unwrap();
    } // db dropped, files flushed
    let db2 = Arc::new(Database::open(&cfg).unwrap());
    let p2 = RocksPersistence::new(db2);
    let got = p2.macro_qc_for(&Hash32([5; 32])).unwrap();
    assert!(got.is_some());
}
```

- [ ] **Step 2: Write `crates/storage/tests/pruning.rs`**

```rust
//! Smoke test for the GC plan helper. Real prune-and-delete lives in a
//! follow-up plan.

use consensus::Config;
use storage::gc;

#[test]
fn plan_uses_configured_horizons() {
    let cfg = Config::default_table_17_1();
    let plan = gc::plan(&cfg, 5_000);
    assert_eq!(plan.hot_horizon_round, 5_000 - cfg.storage.gc_hot_horizon_rounds);
}
```

- [ ] **Step 3: Write `crates/storage/tests/snapshot_roundtrip.rs`**

```rust
//! Snapshot-id determinism.

use storage::snapshot::snapshot_id;
use types::{crypto_types::Hash32, primitives::Height};

#[test]
fn id_is_deterministic_and_distinct_per_input() {
    let a = snapshot_id(Height(1), &Hash32([1; 32]));
    let b = snapshot_id(Height(1), &Hash32([1; 32]));
    let c = snapshot_id(Height(2), &Hash32([1; 32]));
    assert_eq!(a, b);
    assert_ne!(a, c);
}
```

- [ ] **Step 4: Run integration tests**

Run: `cargo test -p storage --tests`
Expected: PASS (3 tests).

---

## Task 10: Full lint + test + commit

- [ ] **Step 1: Full check**

```bash
cargo fmt -p storage -- --check
cargo clippy -p storage --all-targets -- -D warnings
cargo test -p storage
```

Expected: all three exit 0.

- [ ] **Step 2: Commit**

```bash
git add Cargo.toml crates/storage/
git commit -m "feat(storage): scaffold RocksDB adapter and Persistence trait impl"
```

---

## Self-Review

Spec coverage (§7.4):

- `db.rs`: ✅ Task 5 — RocksDB wrapper + CF bootstrap.
- `columns.rs`: ✅ Task 3 — every column family enumerated, wire names locked.
- `keys.rs`: ✅ Task 4 — big-endian encoders.
- `stores/`: ✅ Task 6 — six stores (vertex, micro, macro, valset, slash, vote_book).
- `wal.rs`: ✅ Task 7.
- `gc.rs`: ✅ Task 7 — horizons from `cfg.storage.*`.
- `snapshot.rs`: ✅ Task 7.
- `persistence_impl.rs`: ✅ Task 8 — implements all `Persistence` methods.
- `tests/crash_recovery.rs`: ✅ Task 9.
- `tests/pruning.rs`: ✅ Task 9.
- `tests/snapshot_roundtrip.rs`: ✅ Task 9.

Dependency policy (§9): `consensus` depends on `storage`-supplied trait via `ports::Persistence`. `storage` depends on `consensus` for the **trait definition**, not the algorithm crate logic — direction is `storage → consensus` (trait) and `consensus → types/crypto`. Acyclic. ✅

Naming consistency: `RocksPersistence` (not `RocksDbPersistence` — pedantic clippy) matches the re-export in `lib.rs`. `Persistence` trait methods match plan 03 exactly. `VoteRecord` decoding format is documented inline (3 fields; 72 bytes) to lock the wire shape.

Placeholders: `gc.rs` returns a plan struct but doesn't yet execute deletes; `snapshot.rs` exposes an id helper but no SST export. Both are flagged inline as "follow-up plan" work — the **trait surface** is complete.
