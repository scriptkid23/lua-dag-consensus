# BlobStatus RocksDB Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist `Action::UpdateBlobStatus` to RocksDB with **monotonic** lifecycle semantics (mirror `apps/sim/src/virtual_persistence.rs`), and wire `RocksConsensusQuery::blob_status` + JSON-RPC to read the stored value instead of stubs.

**Architecture:** Add a new column family `blob_status` keyed by `BlobId` (32 bytes) storing a single-byte `BlobStatus` discriminant. Extend `RocksPersistence` with inherent methods `update_blob_status` / `blob_status` (same pattern as sim — **not** on the `Persistence` trait, which covers finalized artifacts only). `ActionApplier` calls `update_blob_status` on each `UpdateBlobStatus` action. `apps/node/src/query.rs` delegates to Rocks.

**Tech Stack:** Rust 1.88, `storage`, `consensus::api::tier::BlobStatus`, `apps/node`.

**Spec:** [`docs/superpowers/specs/2026-05-22-l3-macro-finality-design.md`](../specs/2026-05-22-l3-macro-finality-design.md) §5.7 / `UpdateBlobStatus` wiring; [`docs/superpowers/specs/2026-05-11-folder-architecture-design.md`](../specs/2026-05-11-folder-architecture-design.md) Appendix A lifecycle.

**Prerequisite:** **06b-l3** `ActionApplier` landed (currently logs `"UpdateBlobStatus (not persisted yet)"`).

---

## Current gap

| Area | Today | Target |
|------|-------|--------|
| `ActionApplier` | `debug!` log only | `RocksPersistence::update_blob_status` |
| `RocksConsensusQuery::blob_status` | always `Ok(BlobStatus::Accepted)` | read from RocksDB |
| Storage crate | no blob column | `ColumnFamily::BlobStatus` |
| RPC | no blob query method (optional) | `lua_getBlobStatus` (optional Task 5) |
| Monotonic rule | sim enforces no downgrade | same rule in store |

**Reference implementation:** `apps/sim/src/virtual_persistence.rs`:

```rust
pub fn update_blob_status(&self, blob: BlobId, status: BlobStatus) {
    let mut map = self.blob_status.write().unwrap();
    let entry = map.entry(blob).or_insert(status);
    if status > *entry {
        *entry = status;
    }
}
```

---

## Design decisions (lock-in)

| Topic | Decision |
|-------|----------|
| Column family name | `"blob_status"` — wire-stable; bump requires migration doc |
| Key | `BlobId.0` — 32 bytes (same as `keys::hash`) |
| Value | `BlobStatus` as `u8` discriminant (`repr(u8)` enum) |
| Monotonic updates | Read-modify-write: only write if `new_status > stored` (or key absent) |
| Missing key | `blob_status()` returns `Ok(BlobStatus::Accepted)` — default tier for unknown blobs (matches API expectation) |
| `Persistence` trait | **Do not extend** — blob lifecycle is query/applier concern, not finalized-artifact port |
| `EpochFinalized` | Stored like other tiers when SM emits it; L4 anchor logic remains future work |
| GC | Out of scope — no pruning in this plan |

---

## File map

| File | Action |
|------|--------|
| `crates/storage/src/columns.rs` | add `BlobStatus` variant |
| `crates/storage/src/keys.rs` | add `blob_id(id: &BlobId) -> [u8; 32]` |
| `crates/storage/src/stores/blob_status_store.rs` | **CREATE** get/put_monotonic |
| `crates/storage/src/stores/mod.rs` | export module |
| `crates/storage/src/persistence_impl.rs` | add `update_blob_status`, `blob_status` on `RocksPersistence` |
| `apps/node/src/action_applier.rs` | call `update_blob_status` |
| `apps/node/src/query.rs` | read via `persistence.blob_status` |
| `apps/node/src/rpc_server.rs` | optional `lua_getBlobStatus` |
| `crates/storage/tests/blob_status_store.rs` | **CREATE** roundtrip + monotonic tests |
| `apps/node/tests/blob_status_persist.rs` | **CREATE** applier integration |

---

### Task 1: Column family + key encoding

**Files:**
- Modify: `crates/storage/src/columns.rs`, `crates/storage/src/keys.rs`

- [ ] **Step 1: Add CF**

```rust
// columns.rs
/// `blob_id -> BlobStatus` (single byte).
BlobStatus,
// ...
Self::BlobStatus => "blob_status",
// append to ColumnFamily::all()
```

- [ ] **Step 2: Key helper**

```rust
// keys.rs
use types::primitives::BlobId;

#[must_use]
pub fn blob_id(id: &BlobId) -> [u8; 32] {
    *id.0
}
```

- [ ] **Step 3: Verify DB open** — existing databases without CF will fail open until recreated or migration added. Document in plan: **devnet data dirs may need wipe** (`docker compose down -v`).

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(storage): blob_status column family (BlobStatus persist)"
```

---

### Task 2: `blob_status_store`

**Files:**
- Create: `crates/storage/src/stores/blob_status_store.rs`

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn put_and_get_roundtrip() {
    let db = open_temp_db();
    let blob = BlobId([7; 32]);
    put_monotonic(&db, &blob, BlobStatus::Justified).unwrap();
    assert_eq!(get(&db, &blob).unwrap(), Some(BlobStatus::Justified));
}

#[test]
fn monotonic_no_downgrade() {
    let db = open_temp_db();
    let blob = BlobId([8; 32]);
    put_monotonic(&db, &blob, BlobStatus::Finalized).unwrap();
    put_monotonic(&db, &blob, BlobStatus::SoftConfirmed).unwrap();
    assert_eq!(get(&db, &blob).unwrap(), Some(BlobStatus::Finalized));
}

#[test]
fn upgrade_allowed() {
    let db = open_temp_db();
    let blob = BlobId([9; 32]);
    put_monotonic(&db, &blob, BlobStatus::SoftConfirmed).unwrap();
    put_monotonic(&db, &blob, BlobStatus::Justified).unwrap();
    assert_eq!(get(&db, &blob).unwrap(), Some(BlobStatus::Justified));
}
```

- [ ] **Step 2: Implement**

```rust
pub fn get(db: &Database, blob: &BlobId) -> Result<Option<BlobStatus>> {
    let key = keys::blob_id(blob);
    let Some(bytes) = db.get_raw(ColumnFamily::BlobStatus, &key)? else {
        return Ok(None);
    };
    if bytes.len() != 1 {
        return Err(Error::Logic("blob_status row wrong length"));
    }
    BlobStatus::try_from(bytes[0]).map(Some).map_err(|_| Error::Logic("invalid blob status byte"))
}

pub fn put_monotonic(db: &Database, blob: &BlobId, status: BlobStatus) -> Result<()> {
    let current = get(db, blob)?;
    let should_write = match current {
        None => true,
        Some(existing) => status > existing,
    };
    if should_write {
        let key = keys::blob_id(blob);
        db.put_raw(ColumnFamily::BlobStatus, &key, &[status as u8])?;
    }
    Ok(())
}
```

Add `TryFrom<u8>` for `BlobStatus` in store or use match on discriminant.

- [ ] **Step 3: Run**

```bash
cargo test -p storage blob_status --locked
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(storage): monotonic blob_status store"
```

---

### Task 3: `RocksPersistence` API

**Files:**
- Modify: `crates/storage/src/persistence_impl.rs`

- [ ] **Step 1: Methods**

```rust
impl RocksPersistence {
    pub fn update_blob_status(
        &self,
        blob: &BlobId,
        status: BlobStatus,
    ) -> consensus::Result<()> {
        blob_status_store::put_monotonic(&self.db, blob, status).map_err(|e| map_err(&e))
    }

    pub fn blob_status(&self, blob: &BlobId) -> consensus::Result<BlobStatus> {
        match blob_status_store::get(&self.db, blob).map_err(|e| map_err(&e))? {
            Some(s) => Ok(s),
            None => Ok(BlobStatus::Accepted),
        }
    }
}
```

- [ ] **Step 2: Re-export `BlobStatus`** from `consensus::api::tier` in storage tests only if needed.

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(storage): RocksPersistence blob_status accessors"
```

---

### Task 4: Node wiring

**Files:**
- Modify: `apps/node/src/action_applier.rs`, `apps/node/src/query.rs`

- [ ] **Step 1: ActionApplier**

Replace debug stub:

```rust
Action::UpdateBlobStatus { blob, status } => {
    self.persistence.update_blob_status(blob, *status)?;
    debug!(target: "node::action_applier", ?blob, ?status, "UpdateBlobStatus persisted");
}
```

- [ ] **Step 2: Query**

```rust
fn blob_status(&self, blob: &BlobId) -> Result<BlobStatus> {
    self.persistence.blob_status(blob)
}
```

- [ ] **Step 3: Integration test** `apps/node/tests/blob_status_persist.rs`

```rust
#[test]
fn applier_persists_monotonic_status() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(/* ... */).unwrap());
    let p = RocksPersistence::new(db);
    let mut applier = ActionApplier::new(p.clone(), /* ... */);
    let blob = BlobId([1; 32]);
    applier.apply(&Action::UpdateBlobStatus { blob, status: BlobStatus::SoftConfirmed }).unwrap();
    applier.apply(&Action::UpdateBlobStatus { blob, status: BlobStatus::Justified }).unwrap();
    applier.apply(&Action::UpdateBlobStatus { blob, status: BlobStatus::SoftConfirmed }).unwrap(); // no downgrade
    assert_eq!(p.blob_status(&blob).unwrap(), BlobStatus::Justified);
}
```

- [ ] **Step 4: Run**

```bash
cargo test -p node blob_status --locked
```

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(node): persist UpdateBlobStatus to RocksDB"
```

---

### Task 5 (optional): JSON-RPC `lua_getBlobStatus`

**Files:**
- Modify: `apps/node/src/rpc_server.rs`

- [ ] **Step 1: Method**

```json
{"jsonrpc":"2.0","id":1,"method":"lua_getBlobStatus","params":["0x0101..."]}
```

Params: hex-encoded 32-byte `BlobId`. Response: status string enum (`"accepted"`, `"soft_confirmed"`, …).

- [ ] **Step 2: Test via unit test on handler

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(node): lua_getBlobStatus RPC"
```

---

### Task 6: Acceptance + docs

- [ ] **Step 1: Regression**

```bash
cargo test -p storage -p node -p sim --locked
# sim happy_path still green — sim uses VirtualPersistence, unchanged
```

- [ ] **Step 2: Migration note** in `docker/README.md`:

> Adding `blob_status` CF requires recreating RocksDB data dirs (`docker compose down -v`) on upgrade.

- [ ] **Step 3: Update `2026-05-22-l3-macro-finality-design.md`** — remove "blob status TBD / metrics-only" from 06b-l3 table footnote.

- [ ] **Step 4: Commit**

```bash
git commit -m "docs: blob status persistence landed"
```

---

## Done — acceptance criteria

- Every `UpdateBlobStatus` applied locally updates RocksDB (monotonic).
- `RocksConsensusQuery::blob_status` returns stored tier; unknown blobs → `Accepted`.
- Downgrade attempts are silently ignored (same as sim).
- Existing sim tests unchanged.
- Node integration test proves applier → query roundtrip.

**Non-goals:**

- Cross-validator blob status aggregation (each node stores locally observed status)
- `EpochFinalized` / L4 anchor semantics
- Blob status GC or snapshot export

**Next:** [Devnet E2E smoke](../plans/2026-05-15-devnet-prodlike.md) — compose 4 node + RPC finalized head + optional `lua_getBlobStatus`.
