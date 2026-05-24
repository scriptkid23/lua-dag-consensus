# L1 Real Vertex BLS Certificates (07a) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace fixture vertex certificates with real BLS quorum signatures, verified at the host boundary before any `LiveDag` mutation, via a new shared `crates/dag/` library.

**Architecture:** `crates/dag` exposes canonical vertex signing roots, content hashing, quorum cert construction (devnet multi-signer), and aggregate verification. `apps/node` orchestrator and `L1Driver` call `dag::cert::verify_certified_vertex` before `LiveDag::ingest`. Vertex builders in node and sim delegate cert creation to `dag::cert`. Bullshark/SM unchanged.

**Tech Stack:** Rust 1.88, `blst` BLS (via `crypto`), `borsh`, `types`, `consensus` (error types only).

**Spec:** [`docs/superpowers/specs/2026-05-23-l1-availability-dag-design.md`](../specs/2026-05-23-l1-availability-dag-design.md) §4 (Phase A).

**Prerequisite:** 06b-L1 landed (`L1Driver`, `LiveDag`, gossip `certified-vertex`).

---

## Current gap

| Area | Today | Target |
|------|-------|--------|
| Certificate | `BlsSig([0xAB; 96])`, bitmap `0xFF` | Real aggregate over `2f+1` validator sigs |
| Hash | `SIM_VERTEX_HASH(round ‖ author)` | Content hash over `(round, author, parents, blobs)` |
| Verify | None | Reject bad certs before `LiveDag::ingest` |
| Shared logic | Duplicated in `node/l1/vertex_builder` + `sim/vertex_factory` | `crates/dag/` |

---

## Design decisions (lock-in)

| Topic | Decision |
|-------|----------|
| Signing payload | Borsh of `Vertex` **with `hash = Hash32::zero()`** (explicit zero placeholder) |
| Content hash | `blake3_with_dst(VERTEX_HASH, signing_bytes)` |
| Cert DST | `lua-dag/v1/vertex-cert` |
| Quorum | `2f+1` distinct validators; bitmap indices = position in `valset.entries` |
| Devnet signing | Use `devnet_bls_ikm(label)` per author; labels `node0..node3` mapped from `ValidatorId` |
| Config | `[node].l1_real_vertex_certs: bool` — `true` in devnet, `false` keeps fixture path for unit tests |
| Verify location | **Orchestrator** (all `CertifiedVertexReceived`) + **L1Driver** (local ingest before publish) |
| `net` crate | Does **not** verify (no valset dependency); orchestrator is single policy gate for gossip |

---

## File map

| File | Action |
|------|--------|
| `Cargo.toml` | add workspace member `crates/dag` |
| `crates/dag/Cargo.toml` | **CREATE** |
| `crates/dag/src/lib.rs` | **CREATE** |
| `crates/dag/src/signing.rs` | **CREATE** signing root + content hash |
| `crates/dag/src/cert.rs` | **CREATE** build + verify |
| `crates/dag/src/devnet.rs` | **CREATE** label ↔ ValidatorId helpers |
| `crates/crypto/src/hash.rs` | add `VERTEX_HASH`, `VERTEX_CERT` DSTs |
| `apps/node/Cargo.toml` | depend on `dag` |
| `apps/sim/Cargo.toml` | depend on `dag` |
| `apps/node/src/config_layers.rs` | `l1_real_vertex_certs: bool` |
| `config/profiles/devnet.toml` | `l1_real_vertex_certs = true` |
| `apps/node/src/l1/vertex_builder.rs` | delegate to `dag` when flag true |
| `apps/node/src/l1/driver.rs` | verify before ingest |
| `apps/node/src/orchestrator.rs` | verify before ingest |
| `apps/sim/src/vertex_factory.rs` | delegate to `dag` |
| `apps/node/tests/vertex_cert_reject.rs` | **CREATE** tampered cert rejected |
| `crates/dag/tests/cert_roundtrip.rs` | **CREATE** integration tests |

---

### Task 1: Workspace + `crates/dag` skeleton

**Files:**
- Create: `crates/dag/Cargo.toml`
- Create: `crates/dag/src/lib.rs`
- Modify: root `Cargo.toml`

- [ ] **Step 1: Add `crates/dag/Cargo.toml`**

```toml
[package]
name = "dag"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = false

[dependencies]
borsh = { workspace = true }
crypto = { path = "../crypto" }
types = { path = "../types" }
thiserror = { workspace = true }

[dev-dependencies]
rand = { workspace = true }
rand_chacha = { workspace = true }
```

- [ ] **Step 2: Add `crates/dag/src/lib.rs`**

```rust
//! L1 availability DAG algorithms (certificates, blob custody, erasure).
//!
//! Phase 07a: vertex BLS quorum certificates only.

pub mod cert;
pub mod devnet;
pub mod signing;
```

- [ ] **Step 3: Append workspace member** in root `Cargo.toml`:

```toml
    "crates/dag",
```

- [ ] **Step 4: Verify compile**

Run: `cargo check -p dag --locked`  
Expected: exit 0 (empty modules compile once added in Task 2)

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/dag/
git commit -m "feat(dag): add crates/dag workspace skeleton"
```

---

### Task 2: DST constants + signing root

**Files:**
- Modify: `crates/crypto/src/hash.rs`
- Create: `crates/dag/src/signing.rs`

- [ ] **Step 1: Add DSTs** in `crates/crypto/src/hash.rs` inside `pub mod dst`:

```rust
    /// Production vertex content hash (L1 07a).
    pub const VERTEX_HASH: &[u8] = b"lua-dag/v1/vertex-hash";
    /// BLS quorum certificate domain for certified vertices (L1 07a).
    pub const VERTEX_CERT: &[u8] = b"lua-dag/v1/vertex-cert";
```

- [ ] **Step 2: Failing test** in `crates/dag/src/signing.rs`:

```rust
//! Canonical vertex signing root and content hash.

use borsh::BorshSerialize;
use crypto::hash::{blake3_with_dst, dst};
use types::{
    crypto_types::Hash32,
    dag::{BlobRef, Vertex},
    primitives::{Round, ValidatorId},
};

/// Vertex body for signing: same fields as [`Vertex`] but hash pinned to zero.
#[derive(BorshSerialize)]
struct SignableVertex<'a> {
    round: Round,
    author: ValidatorId,
    parents: &'a [Hash32],
    blobs: &'a [BlobRef],
    hash: Hash32,
}

/// Canonical signing bytes for a vertex (excludes real content hash).
pub fn signing_bytes(vertex: &Vertex) -> Vec<u8> {
    let signable = SignableVertex {
        round: vertex.round,
        author: vertex.author,
        parents: &vertex.parents,
        blobs: &vertex.blobs,
        hash: Hash32([0u8; 32]),
    };
    borsh::to_vec(&signable).expect("vertex signing root must borsh")
}

/// Production content hash for a vertex body.
#[must_use]
pub fn content_hash(vertex: &Vertex) -> Hash32 {
    blake3_with_dst(dst::VERTEX_HASH, &signing_bytes(vertex))
}

/// Attach the content hash to an uncertified vertex (mutates hash field).
pub fn seal_hash(vertex: &mut Vertex) {
    vertex.hash = content_hash(vertex);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_is_deterministic() {
        let mut v = Vertex {
            round: Round(3),
            author: ValidatorId([1u8; 32]),
            parents: vec![Hash32([2u8; 32])],
            blobs: vec![],
            hash: Hash32([0u8; 32]),
        };
        let h1 = content_hash(&v);
        let h2 = content_hash(&v);
        assert_eq!(h1, h2);
        seal_hash(&mut v);
        assert_eq!(v.hash, h1);
    }

    #[test]
    fn changing_parents_changes_hash() {
        let base = Vertex {
            round: Round(1),
            author: ValidatorId([0u8; 32]),
            parents: vec![],
            blobs: vec![],
            hash: Hash32([0u8; 32]),
        };
        let mut with_parent = base.clone();
        with_parent.parents.push(Hash32([9u8; 32]));
        assert_ne!(content_hash(&base), content_hash(&with_parent));
    }
}
```

- [ ] **Step 3: Export module** in `crates/dag/src/lib.rs`:

```rust
pub mod signing;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p dag signing --locked`  
Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/crypto/src/hash.rs crates/dag/src/signing.rs crates/dag/src/lib.rs
git commit -m "feat(dag): vertex signing root and content hash"
```

---

### Task 3: Quorum certificate build + verify

**Files:**
- Create: `crates/dag/src/cert.rs`
- Create: `crates/dag/src/devnet.rs`

- [ ] **Step 1: Devnet helpers** in `crates/dag/src/devnet.rs`:

```rust
//! Devnet-only key lookup for multi-signer quorum certs in phase 07a.

use crypto::hash::{blake3_with_dst, dst};
use types::primitives::ValidatorId;

/// Devnet label for a validator id (`node0`..`node3`).
pub fn devnet_label_for_validator_id(id: &ValidatorId) -> Option<&'static str> {
    for label in ["node0", "node1", "node2", "node3"] {
        let h = blake3_with_dst(dst::DEVNET_PEER_IDENTITY, label.as_bytes());
        if h.0 == id.0 {
            return Some(label);
        }
    }
    None
}

/// BLS IKM for a devnet label (mirrors `apps/node/src/devnet_keys.rs`).
#[must_use]
pub fn devnet_bls_ikm(label: &str) -> [u8; 32] {
    blake3_with_dst(dst::VALIDATOR_BLS_PARTIAL, label.as_bytes()).0
}
```

- [ ] **Step 2: Failing tests** in `crates/dag/tests/cert_roundtrip.rs`:

```rust
use crypto::hash::dst;
use dag::{cert, signing};
use types::{
    crypto_types::Hash32,
    dag::{CertifiedVertex, Vertex},
    primitives::{Round, ValidatorId},
    validator::ValidatorSet,
};

fn devnet_valset() -> ValidatorSet {
    // Copy entries from apps/node devnet_valset_four via inline labels:
    let entries = ["node0", "node1", "node2", "node3"]
        .into_iter()
        .map(|label| {
            let ikm = dag::devnet::devnet_bls_ikm(label);
            let sk = crypto::bls::SecretKey::from_ikm(&ikm).unwrap();
            types::validator::ValidatorEntry {
                id: ValidatorId(
                    crypto::hash::blake3_with_dst(
                        crypto::hash::dst::DEVNET_PEER_IDENTITY,
                        label.as_bytes(),
                    )
                    .0,
                ),
                bls_pubkey: sk.public().to_bytes(),
                vrf_pubkey: types::crypto_types::VrfPubkey([0u8; 32]),
                stake: types::primitives::StakeWeight(1),
                identity: types::validator::ValidatorIdentity {
                    asn: None,
                    cloud: None,
                    region: None,
                },
            }
        })
        .collect();
    types::validator::ValidatorSet {
        epoch: types::primitives::Epoch(0),
        total_stake: types::primitives::StakeWeight(4),
        entries,
    }
}

#[test]
fn build_and_verify_quorum_cert() {
    let valset = devnet_valset();
    let author = valset.entries[0].id;
    let mut vertex = Vertex {
        round: Round(0),
        author,
        parents: vec![],
        blobs: vec![],
        hash: Hash32([0u8; 32]),
    };
    signing::seal_hash(&mut vertex);
    let signer_indices: Vec<u32> = vec![0, 1, 2]; // 2f+1 for n=4
    let cv = cert::build_quorum_cert(&vertex, &valset, &signer_indices).unwrap();
    cert::verify_certified_vertex(&cv, &valset).unwrap();
}

#[test]
fn tampered_hash_fails_verify() {
    let valset = devnet_valset();
    let mut vertex = Vertex {
        round: Round(1),
        author: valset.entries[1].id,
        parents: vec![],
        blobs: vec![],
        hash: Hash32([0u8; 32]),
    };
    signing::seal_hash(&mut vertex);
    let cv = cert::build_quorum_cert(&vertex, &valset, &[0, 1, 2]).unwrap();
    let mut bad = cv.clone();
    bad.vertex.hash = Hash32([0xFFu8; 32]);
    let err = cert::verify_certified_vertex(&bad, &valset).unwrap_err();
    assert!(err.to_string().contains("hash"));
}
```

- [ ] **Step 3: Implement `crates/dag/src/cert.rs`**

```rust
//! Quorum BLS certificates for [`CertifiedVertex`].

use crypto::{
    bls::{
        aggregate::{aggregate_sigs, verify_aggregate},
        bitmap,
        keys::{PublicKey, SecretKey},
        sign::sign,
    },
    hash::dst,
};
use types::{
    crypto_types::{BlsAggSig, BlsSig},
    dag::{CertifiedVertex, Vertex},
    validator::ValidatorSet,
};

use crate::{devnet, signing};

#[derive(Debug, thiserror::Error)]
pub enum CertError {
    #[error("vertex content hash mismatch")]
    HashMismatch,
    #[error("insufficient signers: got {got}, need {need}")]
    InsufficientSigners { got: u32, need: u32 },
    #[error("validator index out of range: {0}")]
    BadIndex(u32),
    #[error("unknown devnet author")]
    UnknownDevnetAuthor,
    #[error("bls: {0}")]
    Bls(#[from] crypto::error::Error),
}

pub type Result<T> = std::result::Result<T, CertError>;

fn quorum_threshold(n: u32) -> u32 {
    let f = n.saturating_sub(1) / 3;
    2 * f + 1
}

/// Build a quorum certificate over `vertex` from `signer_indices` (valset positions).
pub fn build_quorum_cert(
    vertex: &Vertex,
    valset: &ValidatorSet,
    signer_indices: &[u32],
) -> Result<CertifiedVertex> {
    let n = u32::try_from(valset.entries.len()).map_err(|_| CertError::BadIndex(0))?;
    let need = quorum_threshold(n);
    if u32::try_from(signer_indices.len()).unwrap_or(0) < need {
        return Err(CertError::InsufficientSigners {
            got: signer_indices.len() as u32,
            need,
        });
    }
    let msg = signing::signing_bytes(vertex);
    let mut sigs = Vec::with_capacity(signer_indices.len());
    let mut contributors = Vec::with_capacity(signer_indices.len());
    for &idx in signer_indices {
        let entry = valset
            .entries
            .get(idx as usize)
            .ok_or(CertError::BadIndex(idx))?;
        let label = devnet::devnet_label_for_validator_id(&entry.id)
            .ok_or(CertError::UnknownDevnetAuthor)?;
        let sk = SecretKey::from_ikm(&devnet::devnet_bls_ikm(label)).map_err(CertError::Bls)?;
        sigs.push(sign(&sk, dst::VERTEX_CERT, &msg));
        contributors.push(idx);
    }
    let agg = aggregate_sigs(&sigs).map_err(CertError::Bls)?;
    let mut bm = bitmap::Bitmap::new(n as usize);
    for &idx in &contributors {
        bm.set(idx as usize).map_err(|_| CertError::BadIndex(idx))?;
    }
    Ok(CertifiedVertex {
        vertex: vertex.clone(),
        certificate: BlsAggSig {
            sig: agg,
            bitmap: bm.as_bytes().to_vec(),
        },
    })
}

fn bitmap_indices(bitmap: &[u8], n: u32) -> Result<Vec<u32>> {
    let bm = bitmap::Bitmap::from_bytes(bitmap.to_vec(), n as usize)
        .map_err(|_| CertError::BadIndex(0))?;
    let mut out = Vec::new();
    for i in 0..n as usize {
        if bm.get(i).map_err(|_| CertError::BadIndex(i as u32))? {
            out.push(i as u32);
        }
    }
    Ok(out)
}

/// Verify content hash + quorum BLS certificate.
pub fn verify_certified_vertex(cv: &CertifiedVertex, valset: &ValidatorSet) -> Result<()> {
    if cv.vertex.hash != signing::content_hash(&cv.vertex) {
        return Err(CertError::HashMismatch);
    }
    let n = u32::try_from(valset.entries.len()).map_err(|_| CertError::BadIndex(0))?;
    let need = quorum_threshold(n);
    let indices = bitmap_indices(&cv.certificate.bitmap, n)?;
    if u32::try_from(indices.len()).unwrap_or(0) < need {
        return Err(CertError::InsufficientSigners {
            got: indices.len() as u32,
            need,
        });
    }
    let msg = signing::signing_bytes(&cv.vertex);
    let pks: Vec<PublicKey> = indices
        .iter()
        .map(|&idx| {
            let entry = valset
                .entries
                .get(idx as usize)
                .ok_or(CertError::BadIndex(idx))?;
            PublicKey::from_bytes(&entry.bls_pubkey).map_err(CertError::Bls)
        })
        .collect::<Result<_>>()?;
    verify_aggregate(&pks, dst::VERTEX_CERT, &msg, &cv.certificate.sig).map_err(CertError::Bls)?;
    Ok(())
}
```

Add `pub mod cert;` and `pub mod devnet;` to `lib.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p dag cert_roundtrip --locked`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/dag/
git commit -m "feat(dag): quorum vertex cert build and verify"
```

---

### Task 4: Wire node vertex builder + config flag

**Files:**
- Modify: `apps/node/src/config_layers.rs`
- Modify: `config/profiles/devnet.toml`
- Modify: `apps/node/Cargo.toml` (add `dag = { path = "../../crates/dag" }`)
- Modify: `apps/node/src/l1/vertex_builder.rs`

- [ ] **Step 1: Config field** in `NodeSection`:

```rust
    /// When true, build real BLS quorum certs via `dag::cert` (07a).
    #[serde(default)]
    pub l1_real_vertex_certs: bool,
```

Set in `config/profiles/devnet.toml`:

```toml
l1_real_vertex_certs = true
```

- [ ] **Step 2: Update `build_certified_vertex`** to accept `real_certs: bool` and `valset: &ValidatorSet`:

When `real_certs`:
1. Build uncertified `Vertex` with zero hash
2. `dag::signing::seal_hash(&mut vertex)`
3. Compute rotating quorum indices `(round + i) % n` for `i in 0..quorum`
4. `dag::cert::build_quorum_cert(&vertex, valset, &indices)`

When `!real_certs`: keep existing `fixture_certificate()` path unchanged.

- [ ] **Step 3: Thread flag** from `L1Driver::new` (add param `real_vertex_certs: bool`) from `runtime.rs` reading `cfg.node.l1_real_vertex_certs`.

- [ ] **Step 4: Run node unit tests**

Run: `cargo test -p node vertex_builder --locked`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add apps/node/ config/profiles/devnet.toml
git commit -m "feat(node): real vertex certs behind l1_real_vertex_certs"
```

---

### Task 5: Ingress verification gate

**Files:**
- Modify: `apps/node/src/orchestrator.rs`
- Modify: `apps/node/src/l1/driver.rs`
- Modify: `apps/node/src/runtime.rs` (pass valset + flag to orchestrator or store on host bundle)

- [ ] **Step 1: Failing integration test** `apps/node/tests/vertex_cert_reject.rs`:

```rust
//! Tampered certified vertices must not reach the DAG.

use std::sync::Arc;

use dag::cert;
use node::{host_context::StubHostBundle, live_dag::LiveDag};
use storage::{config::StorageConfig, db::Database};
use types::dag::CertifiedVertex;

#[test]
fn verify_rejects_fixture_cert_when_real_certs_enabled() {
    // Build fixture vertex (old 0xAB cert), assert verify fails against devnet valset
    let valset = node::devnet_keys::devnet_valset_four();
    let cv: CertifiedVertex = /* use old fixture builder with real hash sealed */;
    assert!(cert::verify_certified_vertex(&cv, &valset).is_err());
}
```

- [ ] **Step 2: Orchestrator gate** — before `self.host_bundle.dag.ingest(cv)`:

```rust
if self.l1_real_vertex_certs {
    if let Err(e) = dag::cert::verify_certified_vertex(cv, &self.valset) {
        warn!(target: "node::orchestrator", error = %e, "rejecting certified vertex");
        self.metrics.vertex_cert_rejected.inc();
        continue;
    }
}
```

Add `valset: ValidatorSet` and `l1_real_vertex_certs: bool` to `Orchestrator` (from `runtime.rs`).

- [ ] **Step 3: L1Driver gate** — same verify call before `self.dag.ingest` when flag true.

- [ ] **Step 4: Metric** — add `vertex_cert_rejected` counter to `apps/node/src/observability/metrics.rs`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p node vertex_cert l1_driver l1_gossip --locked`  
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add apps/node/
git commit -m "feat(node): verify vertex certs before DAG ingest"
```

---

### Task 6: Update sim factory + regression

**Files:**
- Modify: `apps/sim/Cargo.toml`
- Modify: `apps/sim/src/vertex_factory.rs`

- [ ] **Step 1: Sim uses `dag`** for cert building (always real certs in sim — removes fixture duplication).

- [ ] **Step 2: Run sim + workspace**

Run: `cargo test -p sim happy_path --locked`  
Run: `cargo test --workspace --locked`  
Expected: all PASS

- [ ] **Step 3: Update spec status** in `docs/superpowers/specs/2026-05-23-l1-availability-dag-design.md`:

```markdown
**Status:** Phase A (07a) plan ready
```

- [ ] **Step 4: Commit**

```bash
git add apps/sim/ docs/superpowers/specs/2026-05-23-l1-availability-dag-design.md
git commit -m "feat(sim): use dag crate for vertex certificates"
```

---

## Done — 07a acceptance criteria

- `crates/dag` verifies quorum BLS certs with production hash recipe
- Devnet profile sets `l1_real_vertex_certs = true`
- Fixture certs rejected at orchestrator when flag enabled
- `l1_driver_smoke`, `l1_gossip_roundtrip`, sim `happy_path` green
- `docker compose` E2E finality still reachable (manual or CI)

**Non-goals (explicit):**

- Blob payload / custody (07b)
- Erasure coding / causal-set RPC (07c)
- Per-node distributed cert protocol (devnet central multi-sign OK for 07a)

**Next:** [`2026-05-23-07b-l1-blob-custody.md`](./2026-05-23-07b-l1-blob-custody.md) (to be written after 07a lands)
