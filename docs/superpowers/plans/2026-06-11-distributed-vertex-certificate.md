# Distributed Vertex Certificate Protocol — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **UPDATE (2026-07-06):** The `vertex_protocol` rollout flag and the legacy
> `devnet_factory` path this plan kept as default were removed; distributed is now the
> only L1 production path. See
> [`2026-07-06-remove-devnet-factory-design.md`](../specs/2026-07-06-remove-devnet-factory-design.md).

**Goal:** Implement the approved 2026-06-04 design: a `vertex_cert` module in `crates/consensus` (mirroring `macro_fin`) where each validator proposes its own vertex per round, peers return BLS partials, the proposer aggregates ≥ `2f+1` into a `CertifiedVertex`, and rounds advance cert-driven (Narwhal) — behind a `vertex_protocol` config flag with the legacy devnet factory as default.

**Architecture:** New wire types `VertexProposal`/`VertexPartial` (`crates/types`), new `Event`/`Action` variants routed through the deterministic `StateMachine::step`, per-validator `VertexBook` state in `crates/consensus/src/vertex_cert/`. The proposer (only) collects partials and emits `Action::BroadcastCertifiedVertex`; the host orchestrator loops that cert back locally (gossipsub does not deliver one's own publish). Blob drain moves behind a new `PendingBlobSource` port on `HostContext`. `L1Driver` stays untouched behind `vertex_protocol = "devnet_factory"` (default) so all existing tests stay green.

**Tech Stack:** Rust 1.88, tokio, libp2p gossipsub, borsh, blst BLS (`crates/crypto`), `dag::cert` / `dag::signing`, existing `DevSigner` + devnet valset.

**Spec:** [`docs/superpowers/specs/2026-06-04-distributed-vertex-certificate-design.md`](../specs/2026-06-04-distributed-vertex-certificate-design.md) (Approved). Supersedes the 2026-05-29 spec/plan pair (marked superseded).

---

## Codebase-adaptation decisions (verified against source, locked for this plan)

| Spec said | This plan does | Why |
|---|---|---|
| Types in `crates/types/src/dag.rs` | New file `crates/types/src/dag/proposal.rs` | `dag` is a directory module (`mod.rs` + `vertex.rs`/`certified.rs`/`refs.rs`) |
| `vertex_cert` calls `signing_bytes`/`assemble_cert` | `crates/consensus` gains a `dag = { path = "../dag" }` dependency | `dag` depends only on `types`+`crypto` → acyclic |
| Proposer "self-ingests" its cert | SM seeds `certified_by_round`; host orchestrator **loops back** `Action::BroadcastCertifiedVertex` as a local `Event::CertifiedVertexReceived` | gossipsub never delivers a node's own publish; LiveDag + Bullshark must still see the cert. Idempotent by `(round, author)` key |
| "L1Driver shrinks to a thin L1Bootstrap" | No L1 task at all in distributed mode; orchestrator calls a new `StateMachine::genesis_propose()` once before its event loop | `vertex_cert` state lives inside the SM, which the orchestrator owns; a separate task has nothing left to do |
| "self-vote the partial" | Proposer signs its own partial and inserts it **directly** into `collecting`; it is **not** broadcast | Only the proposer aggregates its own vertex — peers never need our partial |
| Memory bound | Proposals accepted only for rounds in `[current_round-1, current_round+1]`; book pruned below `current_round - 1` after each advance | spec §4 anti-spam item 4 |

**Invariant (unchanged from 05-29):** `apps/node` runtime MUST NOT call `dag::cert::build_quorum_cert` with `devnet_bls_ikm` for valset indices ≠ self on the distributed path. (`vertex_builder.rs` remains legal only behind `vertex_protocol = "devnet_factory"`.)

**Borsh wire compatibility:** every enum extended here (`Event`, `Action`, `SlashEvidence`) gets its new variants appended at the **end** — never reorder existing variants.

---

## File map

| File | Action |
|------|--------|
| `crates/types/src/dag/proposal.rs` | **CREATE** `VertexProposal`, `VertexPartial` |
| `crates/types/src/dag/mod.rs` | export both |
| `crates/types/tests/codec_roundtrip.rs` | append roundtrip tests |
| `crates/crypto/src/hash.rs` | append `dst::VERTEX_PROPOSAL` |
| `crates/dag/src/cert.rs` | **MODIFY** extract `assemble_cert`, make `quorum_threshold` pub |
| `crates/dag/tests/cert_from_partials.rs` | **CREATE** |
| `crates/types/src/slashing.rs` | append `VertexEquivocation` + `SlashEvidence::VertexEquivocation` |
| `crates/consensus/Cargo.toml` | add `dag` dependency |
| `crates/consensus/src/slashing/vertex_equivocation.rs` | **CREATE** detect + verify |
| `crates/consensus/src/slashing/{mod,evidence}.rs` | wire new module/arm |
| `crates/consensus/src/event.rs` | append 2 `Event` variants |
| `crates/consensus/src/action.rs` | append 3 `Action` variants |
| `crates/consensus/src/ports/pending_blobs.rs` | **CREATE** `PendingBlobSource` + `NoPendingBlobs` |
| `crates/consensus/src/ports/mod.rs`, `host_context.rs` | wire new port |
| `crates/consensus/src/vertex_cert/{mod,book,verify}.rs` | **CREATE** the protocol module |
| `crates/consensus/src/state_machine.rs` | `vertices: VertexBook`, route events, `genesis_propose()` |
| `crates/consensus/src/lib.rs` | `pub mod vertex_cert;` |
| `crates/consensus/tests/vertex_cert_distributed.rs` | **CREATE** 4-validator step-level handshake |
| `crates/net/src/gossip/topics.rs` | `VertexProposal`/`VertexPartial` topics |
| `crates/net/src/gossip_wire.rs` | outbound/inbound/is_broadcast arms + tests |
| `crates/net/src/swarm_runner.rs` | `subscribe_set` additions |
| `crates/net/src/bridge.rs` | exhaustive-match arms |
| `apps/sim/src/virtual_net.rs` | 3 `enqueue_from_action` arms |
| `apps/sim/src/world.rs` | 3 `apply_actions` arms; distributed-mode tick; `pending_blobs` in ctx |
| `apps/sim/src/{args,scenarios/mod}.rs` | new scenario variant |
| `apps/sim/src/scenarios/vertex_cert_distributed.rs` | **CREATE** happy/partition/equivocation |
| `apps/cli/src/stub_context.rs` | `pending_blobs` stub |
| `apps/node/src/config_layers.rs` | `vertex_protocol` enum flag |
| `apps/node/src/host_context.rs` | `CustodyPendingBlobs`; `StubHostBundle` gains custody param |
| `apps/node/src/runtime.rs` | reorder bundle construction; gate by `vertex_protocol` |
| `apps/node/src/orchestrator.rs` | genesis kick + cert loopback |
| `apps/node/tests/l1_distributed_smoke.rs` | **CREATE** |
| `config/profiles/devnet.toml` | explicit `vertex_protocol` line |
| `docs/superpowers/specs/...`, old 06b plan | status updates |

**Suggested PR split:** PR1 = Tasks 1–5 (types, cert, slashing, enum plumbing, port — no behavior change). PR2 = Tasks 6–10 (vertex_cert + SM). PR3 = Tasks 11–13 (host, sim, docs).

---

## Task 1: Wire types `VertexProposal` + `VertexPartial` + new DST

**Files:**
- Create: `crates/types/src/dag/proposal.rs`
- Modify: `crates/types/src/dag/mod.rs`
- Modify: `crates/crypto/src/hash.rs`
- Test: `crates/types/tests/codec_roundtrip.rs` (append)

- [ ] **Step 1: Write the failing roundtrip test**

Append to `crates/types/tests/codec_roundtrip.rs`:

```rust
#[test]
fn vertex_proposal_roundtrips_borsh() {
    use types::dag::{Vertex, VertexProposal};
    let p = VertexProposal {
        vertex: Vertex {
            round: types::primitives::Round(3),
            author: types::primitives::ValidatorId([7; 32]),
            parents: vec![types::crypto_types::Hash32([1; 32])],
            blobs: vec![],
            hash: types::crypto_types::Hash32([2; 32]),
        },
        proposer_sig: types::crypto_types::BlsSig([9; 96]),
    };
    let bytes = borsh::to_vec(&p).unwrap();
    let back: VertexProposal = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, p);
}

#[test]
fn vertex_partial_roundtrips_borsh() {
    use types::dag::VertexPartial;
    let bp = VertexPartial {
        vertex_hash: types::crypto_types::Hash32([2; 32]),
        round: types::primitives::Round(3),
        author: types::primitives::ValidatorId([7; 32]),
        voter: types::primitives::ValidatorId([8; 32]),
        sig: types::crypto_types::BlsSig([9; 96]),
    };
    let bytes = borsh::to_vec(&bp).unwrap();
    let back: VertexPartial = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, bp);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p types codec_roundtrip --locked`
Expected: compile FAIL — `VertexProposal` not found.

- [ ] **Step 3: Create `crates/types/src/dag/proposal.rs`**

```rust
//! L1 distributed-certification wire messages (2026-06-04 design).
//!
//! `Serialize`/`Deserialize` are not derived because `BlsSig` is
//! wire-only (Borsh) — same convention as [`super::CertifiedVertex`].

use borsh::{BorshDeserialize, BorshSerialize};

use super::vertex::Vertex;
use crate::{
    crypto_types::{BlsSig, Hash32},
    primitives::{Round, ValidatorId},
};

/// Header a node proposes for its own round (not yet certified).
///
/// `proposer_sig` signs `dag::signing::signing_bytes(vertex)` under
/// DST `lua-dag/v1/vertex-proposal` (propose authority — distinct from
/// the partial vote so a vote can never be replayed as a proposal).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct VertexProposal {
    /// The proposed vertex (`author = proposer`, sealed `hash`).
    pub vertex: Vertex,
    /// Proposer authority signature (DST `VERTEX_PROPOSAL`).
    pub proposer_sig: BlsSig,
}

/// A single validator's partial vote on a proposal.
///
/// `sig` signs `dag::signing::signing_bytes(vertex)` under DST
/// `lua-dag/v1/vertex-cert` — exactly the message
/// `dag::cert::verify_certified_vertex` checks, so aggregation is just
/// signature collection + bitmap.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct VertexPartial {
    /// Hash of the voted vertex.
    pub vertex_hash: Hash32,
    /// Round of the voted vertex (routing / memory bound).
    pub round: Round,
    /// Proposal owner — only this validator aggregates the partials.
    pub author: ValidatorId,
    /// The validator signing this partial.
    pub voter: ValidatorId,
    /// BLS partial signature (DST `VERTEX_CERT`).
    pub sig: BlsSig,
}
```

- [ ] **Step 4: Export from `crates/types/src/dag/mod.rs`**

```rust
pub mod certified;
pub mod proposal;
pub mod refs;
pub mod vertex;

pub use certified::CertifiedVertex;
pub use proposal::{VertexPartial, VertexProposal};
pub use refs::{BlobRef, ChunkRef};
pub use vertex::Vertex;
```

- [ ] **Step 5: Append the new DST to `crates/crypto/src/hash.rs`**

At the **end** of `pub mod dst` (after `BLOB_MERKLE_NODE`):

```rust
    /// Vertex proposal authority signature (L1 distributed cert, 06-04).
    pub const VERTEX_PROPOSAL: &[u8] = b"lua-dag/v1/vertex-proposal";
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p types codec_roundtrip --locked && cargo check -p crypto --locked`
Expected: both new tests PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/types/src/dag/ crates/types/tests/codec_roundtrip.rs crates/crypto/src/hash.rs
git commit -m "feat(types): VertexProposal and VertexPartial wire types + VERTEX_PROPOSAL DST"
```

---

## Task 2: `dag::cert` — extract `assemble_cert`, publish `quorum_threshold`

**Files:**
- Modify: `crates/dag/src/cert.rs`
- Test: `crates/dag/tests/cert_from_partials.rs` (create)

- [ ] **Step 1: Write the failing tests**

Create `crates/dag/tests/cert_from_partials.rs`:

```rust
//! `assemble_cert` builds a verifying CV from externally collected partials.

use crypto::{bls::keys::SecretKey, bls::sign::sign, hash::dst};
use dag::{cert, signing};
use types::{
    crypto_types::{Hash32, VrfPubkey},
    dag::Vertex,
    primitives::{Epoch, Round, StakeWeight, ValidatorId},
    validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
};

fn sk(i: u8) -> SecretKey {
    SecretKey::from_ikm(&[i; 32]).unwrap()
}

fn vset(n: u8) -> ValidatorSet {
    let entries = (0..n)
        .map(|i| ValidatorEntry {
            id: ValidatorId([i; 32]),
            bls_pubkey: sk(i).public().to_bytes(),
            vrf_pubkey: VrfPubkey::zero(),
            stake: StakeWeight(1),
            identity: ValidatorIdentity {
                asn: None,
                cloud: None,
                region: None,
            },
        })
        .collect();
    ValidatorSet {
        epoch: Epoch(0),
        entries,
        total_stake: StakeWeight(u64::from(n)),
    }
}

fn sealed_vertex(author: ValidatorId) -> Vertex {
    let mut v = Vertex {
        round: Round(1),
        author,
        parents: vec![Hash32([0xAA; 32])],
        blobs: vec![],
        hash: Hash32::zero(),
    };
    signing::seal_hash(&mut v);
    v
}

#[test]
fn quorum_threshold_is_public_and_correct() {
    assert_eq!(cert::quorum_threshold(4), 3);
    assert_eq!(cert::quorum_threshold(1), 1);
    assert_eq!(cert::quorum_threshold(7), 5);
}

#[test]
fn assemble_cert_from_three_partials_verifies() {
    let set = vset(4);
    let vertex = sealed_vertex(set.entries[0].id);
    let msg = signing::signing_bytes(&vertex);
    let contributors: Vec<(u32, types::crypto_types::BlsSig)> = [0u8, 1, 2]
        .iter()
        .map(|&i| (u32::from(i), sign(&sk(i), dst::VERTEX_CERT, &msg)))
        .collect();
    let cv = cert::assemble_cert(&vertex, &set, &contributors).unwrap();
    cert::verify_certified_vertex(&cv, &set).expect("assembled cert must verify");
}

#[test]
fn assemble_cert_below_quorum_fails() {
    let set = vset(4);
    let vertex = sealed_vertex(set.entries[0].id);
    let msg = signing::signing_bytes(&vertex);
    let contributors = vec![(0u32, sign(&sk(0), dst::VERTEX_CERT, &msg))];
    assert!(matches!(
        cert::assemble_cert(&vertex, &set, &contributors),
        Err(cert::CertError::InsufficientSigners { got: 1, need: 3 })
    ));
}

#[test]
fn assemble_cert_rejects_out_of_range_index() {
    let set = vset(4);
    let vertex = sealed_vertex(set.entries[0].id);
    let msg = signing::signing_bytes(&vertex);
    let contributors: Vec<_> = [0u8, 1, 2]
        .iter()
        .map(|&i| (u32::from(i), sign(&sk(i), dst::VERTEX_CERT, &msg)))
        .chain(std::iter::once((
            9u32,
            sign(&sk(3), dst::VERTEX_CERT, &msg),
        )))
        .collect();
    assert!(matches!(
        cert::assemble_cert(&vertex, &set, &contributors),
        Err(cert::CertError::BadIndex(9))
    ));
}

#[test]
fn build_quorum_cert_with_still_works_after_refactor() {
    let set = vset(4);
    let vertex = sealed_vertex(set.entries[0].id);
    let cv = cert::build_quorum_cert_with(&vertex, &set, &[0, 1, 2], |i| {
        Ok(sk(u8::try_from(i).unwrap()))
    })
    .unwrap();
    cert::verify_certified_vertex(&cv, &set).expect("legacy path must still verify");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p dag --test cert_from_partials --locked`
Expected: compile FAIL — `assemble_cert` not found / `quorum_threshold` private.

- [ ] **Step 3: Refactor `crates/dag/src/cert.rs`**

Make the threshold public (replace the private fn at line 36):

```rust
/// `2f+1` quorum threshold for `n` validators.
#[must_use]
pub fn quorum_threshold(n: u32) -> u32 {
    let f = n.saturating_sub(1) / 3;
    2 * f + 1
}
```

Add `assemble_cert` and rewrite `build_quorum_cert_with` to delegate to it (replace the existing body from `let msg = ...` down to the final `Ok(CertifiedVertex {...})`):

```rust
/// Assemble a [`CertifiedVertex`] from externally collected partial
/// signatures (`contributors` = valset index + sig over
/// [`signing::signing_bytes`] under [`dst::VERTEX_CERT`]).
///
/// Performs no signature verification — callers must verify each partial
/// on receipt and SHOULD run [`verify_certified_vertex`] on the result.
pub fn assemble_cert(
    vertex: &Vertex,
    valset: &ValidatorSet,
    contributors: &[(u32, types::crypto_types::BlsSig)],
) -> Result<CertifiedVertex> {
    let n = u32::try_from(valset.entries.len()).map_err(|_| CertError::BadIndex(0))?;
    let need = quorum_threshold(n);
    if u32::try_from(contributors.len()).unwrap_or(0) < need {
        return Err(CertError::InsufficientSigners {
            got: u32::try_from(contributors.len()).unwrap_or(0),
            need,
        });
    }
    let mut sigs = Vec::with_capacity(contributors.len());
    let mut bm = Bitmap::new(n as usize);
    for (idx, sig) in contributors {
        if valset.entries.get(*idx as usize).is_none() {
            return Err(CertError::BadIndex(*idx));
        }
        sigs.push(*sig);
        bm.set(*idx as usize).map_err(|_| CertError::BadIndex(*idx))?;
    }
    let agg = aggregate_sigs(&sigs).map_err(CertError::Bls)?;
    Ok(CertifiedVertex {
        vertex: vertex.clone(),
        certificate: BlsAggSig {
            sig: agg,
            bitmap: bm.as_bytes().to_vec(),
        },
    })
}

/// Build a quorum certificate using a caller-supplied secret-key resolver.
pub fn build_quorum_cert_with<F>(
    vertex: &Vertex,
    valset: &ValidatorSet,
    signer_indices: &[u32],
    mut sk_at: F,
) -> Result<CertifiedVertex>
where
    F: FnMut(u32) -> Result<SecretKey>,
{
    let msg = signing::signing_bytes(vertex);
    let mut contributors = Vec::with_capacity(signer_indices.len());
    for &idx in signer_indices {
        if valset.entries.get(idx as usize).is_none() {
            return Err(CertError::BadIndex(idx));
        }
        let sk = sk_at(idx)?;
        contributors.push((idx, sign(&sk, dst::VERTEX_CERT, &msg)));
    }
    assemble_cert(vertex, valset, &contributors)
}
```

Note: `types::crypto_types::BlsSig` is `Copy` (it is `[u8; 96]` behind `Clone, Copy`), so `*sig` / `*prev != bp.sig` comparisons used later are fine.

- [ ] **Step 4: Run the new and existing dag tests**

Run: `cargo test -p dag --locked`
Expected: all PASS (including pre-existing `cert_roundtrip`).

- [ ] **Step 5: Commit**

```bash
git add crates/dag/src/cert.rs crates/dag/tests/cert_from_partials.rs
git commit -m "feat(dag): assemble_cert from collected partials; pub quorum_threshold"
```

---

## Task 3: `SlashEvidence::VertexEquivocation`

**Files:**
- Modify: `crates/types/src/slashing.rs`
- Modify: `crates/consensus/Cargo.toml` (add `dag` dependency)
- Create: `crates/consensus/src/slashing/vertex_equivocation.rs`
- Modify: `crates/consensus/src/slashing/mod.rs`, `crates/consensus/src/slashing/evidence.rs`

- [ ] **Step 1: Add the evidence type (`crates/types/src/slashing.rs`)**

Add to the imports:

```rust
use crate::dag::{Vertex, VertexProposal};
```

Append the struct before the `SlashEvidence` enum:

```rust
/// Two vertex proposals signed by the same author at the same round
/// with different content hashes (L1 double-propose, 06-04 design).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct VertexEquivocation {
    /// Offender (the vertex author).
    pub validator: ValidatorId,
    /// First conflicting vertex + proposer signature.
    pub a: (Vertex, BlsSig),
    /// Second conflicting vertex + proposer signature.
    pub b: (Vertex, BlsSig),
}

impl VertexEquivocation {
    /// Build evidence from two conflicting proposals at the same round.
    #[must_use]
    pub fn from_proposals(validator: ValidatorId, a: VertexProposal, b: VertexProposal) -> Self {
        Self {
            validator,
            a: (a.vertex, a.proposer_sig),
            b: (b.vertex, b.proposer_sig),
        }
    }
}
```

Append the variant at the **end** of `SlashEvidence` (Borsh append-only):

```rust
    /// Equivocation on an L1 vertex proposal (100 %).
    VertexEquivocation(VertexEquivocation),
```

- [ ] **Step 2: Add the `dag` dependency to `crates/consensus/Cargo.toml`**

In `[dependencies]`, after `crypto`:

```toml
dag         = { path = "../dag" }
```

(`dag` depends only on `types` + `crypto`; no cycle.)

- [ ] **Step 3: Write the failing verifier test**

Create `crates/consensus/src/slashing/vertex_equivocation.rs` with tests first (the module body comes in Step 4 — write the whole file in one edit, tests included):

```rust
//! L1 vertex double-propose detector (100 % slash, 06-04 design).

use crypto::{bls::PublicKey, bls::sign::verify as bls_verify, hash::dst};
use types::{
    dag::VertexProposal,
    primitives::ValidatorId,
    slashing::VertexEquivocation,
    validator::ValidatorSet,
};

use crate::error::Result;

/// Verify a vertex-equivocation evidence bundle.
///
/// Valid evidence = same author, same round, different content hash,
/// both proposer signatures valid under [`dst::VERTEX_PROPOSAL`] over
/// [`dag::signing::signing_bytes`].
pub fn verify(ev: &VertexEquivocation, set: &ValidatorSet) -> Result<()> {
    let entry = set
        .entries
        .iter()
        .find(|e| e.id == ev.validator)
        .ok_or_else(|| crate::Error::InvalidConfig("unknown validator".into()))?;
    let pk = PublicKey::from_bytes(&entry.bls_pubkey)
        .map_err(|_| crate::Error::InvalidConfig("invalid bls pubkey".into()))?;

    if ev.a.0.author != ev.validator || ev.b.0.author != ev.validator {
        return Err(crate::Error::InvalidConfig(
            "equivocation vertices must be authored by the offender".into(),
        ));
    }
    if ev.a.0.round != ev.b.0.round || ev.a.0.hash == ev.b.0.hash {
        return Err(crate::Error::InvalidConfig(
            "equivocation vertices must share round and differ in hash".into(),
        ));
    }

    for (vertex, sig) in [&ev.a, &ev.b] {
        let msg = dag::signing::signing_bytes(vertex);
        bls_verify(&pk, dst::VERTEX_PROPOSAL, &msg, sig)
            .map_err(|_| crate::Error::InvalidConfig("invalid proposer sig".into()))?;
    }
    Ok(())
}

/// Build evidence from two conflicting proposals at the same round.
#[must_use]
pub fn detect(
    validator: ValidatorId,
    a: VertexProposal,
    b: VertexProposal,
) -> VertexEquivocation {
    VertexEquivocation::from_proposals(validator, a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::bls::keys::SecretKey;
    use crypto::bls::sign::sign;
    use types::{
        crypto_types::{Hash32, VrfPubkey},
        dag::Vertex,
        primitives::{Epoch, Round, StakeWeight},
        validator::{ValidatorEntry, ValidatorIdentity},
    };

    fn sk() -> SecretKey {
        SecretKey::from_ikm(&[0x11; 32]).unwrap()
    }

    fn set_with(id: ValidatorId) -> ValidatorSet {
        ValidatorSet {
            epoch: Epoch(0),
            entries: vec![ValidatorEntry {
                id,
                bls_pubkey: sk().public().to_bytes(),
                vrf_pubkey: VrfPubkey::zero(),
                stake: StakeWeight(1),
                identity: ValidatorIdentity {
                    asn: None,
                    cloud: None,
                    region: None,
                },
            }],
            total_stake: StakeWeight(1),
        }
    }

    fn signed_proposal(author: ValidatorId, parent: u8) -> VertexProposal {
        let mut vertex = Vertex {
            round: Round(2),
            author,
            parents: vec![Hash32([parent; 32])],
            blobs: vec![],
            hash: Hash32::zero(),
        };
        dag::signing::seal_hash(&mut vertex);
        let sig = sign(
            &sk(),
            dst::VERTEX_PROPOSAL,
            &dag::signing::signing_bytes(&vertex),
        );
        VertexProposal {
            vertex,
            proposer_sig: sig,
        }
    }

    #[test]
    fn valid_double_propose_evidence_verifies() {
        let author = ValidatorId([1; 32]);
        let ev = detect(author, signed_proposal(author, 1), signed_proposal(author, 2));
        verify(&ev, &set_with(author)).expect("valid evidence");
    }

    #[test]
    fn same_hash_pair_is_rejected() {
        let author = ValidatorId([1; 32]);
        let p = signed_proposal(author, 1);
        let ev = detect(author, p.clone(), p);
        assert!(verify(&ev, &set_with(author)).is_err());
    }

    #[test]
    fn forged_sig_is_rejected() {
        let author = ValidatorId([1; 32]);
        let mut bad = signed_proposal(author, 2);
        bad.proposer_sig = types::crypto_types::BlsSig([0xEE; 96]);
        let ev = detect(author, signed_proposal(author, 1), bad);
        assert!(verify(&ev, &set_with(author)).is_err());
    }
}
```

- [ ] **Step 4: Wire the module**

`crates/consensus/src/slashing/mod.rs`:

```rust
pub mod double_vote;
pub mod equivocation;
pub mod evidence;
pub mod inactivity_leak;
pub mod penalty;
pub mod surround;
pub mod vertex_equivocation;

pub use evidence::verify_evidence;
pub use penalty::Penalty;
```

`crates/consensus/src/slashing/evidence.rs` — add the match arm:

```rust
pub fn verify_evidence(ev: &SlashEvidence, set: &ValidatorSet) -> Result<()> {
    match ev {
        SlashEvidence::MacroEquivocation(e) => equivocation::verify(e, set),
        SlashEvidence::Surround(e) => surround::verify(e, set),
        SlashEvidence::DoubleVote(e) => double_vote::verify(e, set),
        SlashEvidence::VertexEquivocation(e) => vertex_equivocation::verify(e, set),
    }
}
```

(also add `vertex_equivocation` to the `use super::{...}` list.)

- [ ] **Step 5: Run**

Run: `cargo test -p consensus slashing --locked && cargo test -p types --locked`
Expected: 3 new tests PASS; everything else green.

- [ ] **Step 6: Commit**

```bash
git add crates/types/src/slashing.rs crates/consensus/Cargo.toml crates/consensus/src/slashing/
git commit -m "feat(consensus): SlashEvidence::VertexEquivocation with verifier"
```

---

## Task 4: New `Event`/`Action` variants + full wire plumbing

This task appends the enum variants and updates **every exhaustive match in the workspace** in one commit so the build never breaks: `net` (gossip_wire, bridge, topics, swarm), `sim` (world, virtual_net), and a temporary no-op route in `state_machine.rs` (replaced in Task 10).

**Files:**
- Modify: `crates/consensus/src/event.rs`, `crates/consensus/src/action.rs`, `crates/consensus/src/state_machine.rs`
- Modify: `crates/net/src/gossip/topics.rs`, `crates/net/src/gossip_wire.rs`, `crates/net/src/swarm_runner.rs`, `crates/net/src/bridge.rs`
- Modify: `apps/sim/src/virtual_net.rs`, `apps/sim/src/world.rs`

- [ ] **Step 1: Write the failing net tests**

Append to the `#[cfg(test)] mod tests` in `crates/net/src/gossip_wire.rs`:

```rust
    fn vertex_proposal_fixture() -> types::dag::VertexProposal {
        use types::dag::{Vertex, VertexProposal};
        VertexProposal {
            vertex: Vertex {
                round: types::primitives::Round(1),
                author: ValidatorId([4; 32]),
                parents: vec![],
                blobs: vec![],
                hash: Hash32([5; 32]),
            },
            proposer_sig: BlsSig([6; 96]),
        }
    }

    #[test]
    fn vertex_proposal_roundtrips_on_wire() {
        let p = vertex_proposal_fixture();
        let (topic, bytes) =
            outbound_broadcast(&Action::BroadcastVertexProposal(p.clone()))
                .unwrap()
                .unwrap();
        assert_eq!(topic, Topic::VertexProposal);
        let ev = inbound_message(&topic.ident().to_string(), &bytes)
            .unwrap()
            .unwrap();
        assert!(matches!(ev, Event::VertexProposalReceived(got) if got == p));
    }

    #[test]
    fn vertex_partial_roundtrips_on_wire() {
        let bp = types::dag::VertexPartial {
            vertex_hash: Hash32([5; 32]),
            round: types::primitives::Round(1),
            author: ValidatorId([4; 32]),
            voter: ValidatorId([2; 32]),
            sig: BlsSig([6; 96]),
        };
        let (topic, bytes) =
            outbound_broadcast(&Action::BroadcastVertexPartial(bp.clone()))
                .unwrap()
                .unwrap();
        assert_eq!(topic, Topic::VertexPartial);
        let ev = inbound_message(&topic.ident().to_string(), &bytes)
            .unwrap()
            .unwrap();
        assert!(matches!(ev, Event::VertexPartialReceived(got) if got == bp));
    }

    #[test]
    fn broadcast_certified_vertex_action_maps_to_certified_vertex_topic() {
        use types::dag::{CertifiedVertex, Vertex};
        let cv = CertifiedVertex {
            vertex: Vertex {
                round: types::primitives::Round(1),
                author: ValidatorId([4; 32]),
                parents: vec![],
                blobs: vec![],
                hash: Hash32([5; 32]),
            },
            certificate: BlsAggSig {
                sig: BlsSig([0; 96]),
                bitmap: vec![0xFF],
            },
        };
        let (topic, bytes) =
            outbound_broadcast(&Action::BroadcastCertifiedVertex(cv.clone()))
                .unwrap()
                .unwrap();
        assert_eq!(topic, Topic::CertifiedVertex);
        let ev = inbound_message(&topic.ident().to_string(), &bytes)
            .unwrap()
            .unwrap();
        assert!(matches!(ev, Event::CertifiedVertexReceived(got) if got == cv));
    }
```

(The existing test module already imports `Action`/`Event`/`Topic`/`outbound_broadcast`/`inbound_message`; add whichever of these the module does not yet import.)

Run: `cargo test -p net gossip_wire --locked` → compile FAIL (variants missing).

- [ ] **Step 2: Append `Event` variants (`crates/consensus/src/event.rs`)**

Extend the `types::dag` import to `use types::dag::{CertifiedVertex, VertexPartial, VertexProposal};` and append at the **end** of `pub enum Event`:

```rust
    /// A vertex proposal header arrived from a peer (L1 distributed cert).
    VertexProposalReceived(VertexProposal),
    /// A vertex partial vote arrived (routed to the proposal's author).
    VertexPartialReceived(VertexPartial),
```

- [ ] **Step 3: Append `Action` variants (`crates/consensus/src/action.rs`)**

Add `use types::dag::{CertifiedVertex, VertexPartial, VertexProposal};` and append at the **end** of `pub enum Action`:

```rust
    /// Broadcast this node's own vertex proposal (L1 distributed cert).
    BroadcastVertexProposal(VertexProposal),
    /// Broadcast a partial vote on a peer's vertex proposal.
    BroadcastVertexPartial(VertexPartial),
    /// Broadcast a fully aggregated certified vertex. The host MUST also
    /// loop this back locally as `Event::CertifiedVertexReceived` —
    /// gossipsub does not deliver one's own publish.
    BroadcastCertifiedVertex(CertifiedVertex),
```

- [ ] **Step 4: Temporary no-op route in `state_machine.rs`**

`Event` match in `step` is exhaustive; add (replaced by real routing in Task 10):

```rust
            // Routed to vertex_cert in Task 10 of plan 2026-06-11.
            Event::VertexProposalReceived(_) | Event::VertexPartialReceived(_) => {
                Ok(Actions::new())
            }
```

- [ ] **Step 5: Topics (`crates/net/src/gossip/topics.rs`)**

`pub mod wire` — append:

```rust
    /// Vertex proposal headers (L1 distributed cert, 06-04).
    pub const VERTEX_PROPOSAL: &str = "lua-dag/v1/vertex-proposal";
    /// Vertex partial votes (L1 distributed cert, 06-04).
    pub const VERTEX_PARTIAL: &str = "lua-dag/v1/vertex-partial";
```

`pub enum Topic` — append variants:

```rust
    /// Vertex proposal headers (L1 distributed cert).
    VertexProposal,
    /// Vertex partial votes (L1 distributed cert).
    VertexPartial,
```

`wire_name` — add arms:

```rust
            Self::VertexProposal => wire::VERTEX_PROPOSAL.to_string(),
            Self::VertexPartial => wire::VERTEX_PARTIAL.to_string(),
```

`from_wire_name` — add arms (before the `BLS_PARTIAL_PREFIX` guard):

```rust
            wire::VERTEX_PROPOSAL => Some(Self::VertexProposal),
            wire::VERTEX_PARTIAL => Some(Self::VertexPartial),
```

- [ ] **Step 6: `crates/net/src/gossip_wire.rs`**

Add `use types::dag::{VertexPartial, VertexProposal};`. In `outbound_broadcast`, add arms before the no-wire group:

```rust
        Action::BroadcastVertexProposal(p) => {
            (Topic::VertexProposal, encode_action_payload(p)?)
        }
        Action::BroadcastVertexPartial(bp) => {
            (Topic::VertexPartial, encode_action_payload(bp)?)
        }
        Action::BroadcastCertifiedVertex(cv) => {
            (Topic::CertifiedVertex, encode_action_payload(cv)?)
        }
```

In `is_broadcast`, extend the `matches!` list:

```rust
            | Action::BroadcastVertexProposal(_)
            | Action::BroadcastVertexPartial(_)
            | Action::BroadcastCertifiedVertex(_)
```

In `inbound_message`, add arms:

```rust
        Topic::VertexProposal => {
            let p: VertexProposal = decode_event_payload(data)?;
            Ok(Some(Event::VertexProposalReceived(p)))
        }
        Topic::VertexPartial => {
            let bp: VertexPartial = decode_event_payload(data)?;
            Ok(Some(Event::VertexPartialReceived(bp)))
        }
```

- [ ] **Step 7: `crates/net/src/swarm_runner.rs` — subscribe**

In `subscribe_set`, extend the initial `vec![...]` with:

```rust
        Topic::VertexProposal,
        Topic::VertexPartial,
```

- [ ] **Step 8: `crates/net/src/bridge.rs` — exhaustive match**

In `translate_action`, add the three new variants to the **first** arm group (broadcast skeleton-warn group), alongside `Action::BroadcastMacroQc(_)`:

```rust
            | Action::BroadcastVertexProposal(_)
            | Action::BroadcastVertexPartial(_)
            | Action::BroadcastCertifiedVertex(_)
```

- [ ] **Step 9: `apps/sim/src/virtual_net.rs` — event mapping**

In `enqueue_from_action`, add arms before `_ => return`:

```rust
            Action::BroadcastVertexProposal(p) => Event::VertexProposalReceived(p.clone()),
            Action::BroadcastVertexPartial(bp) => Event::VertexPartialReceived(bp.clone()),
            Action::BroadcastCertifiedVertex(cv) => {
                Event::CertifiedVertexReceived(cv.clone())
            }
```

- [ ] **Step 10: `apps/sim/src/world.rs` — exhaustive `apply_actions`**

Add arms to the `match action` in `apply_actions` (the full, final versions — Task 12 does not revisit them):

```rust
                Action::BroadcastVertexProposal(p) => {
                    self.net.enqueue_from_action(
                        validator_idx,
                        &Action::BroadcastVertexProposal(p),
                        n,
                        now,
                        &mut self.rng,
                    );
                }
                Action::BroadcastVertexPartial(bp) => {
                    self.net.enqueue_from_action(
                        validator_idx,
                        &Action::BroadcastVertexPartial(bp),
                        n,
                        now,
                        &mut self.rng,
                    );
                }
                Action::BroadcastCertifiedVertex(cv) => {
                    // Shared DAG ingest + self-delivery: gossip skips the
                    // sender, but the proposer must also see its own cert
                    // (mirrors the node orchestrator loopback).
                    self.dag.insert(cv.clone());
                    self.step_validator(
                        validator_idx,
                        consensus::Event::CertifiedVertexReceived(cv.clone()),
                        now,
                    );
                    self.net.enqueue_from_action(
                        validator_idx,
                        &Action::BroadcastCertifiedVertex(cv),
                        n,
                        now,
                        &mut self.rng,
                    );
                }
```

(`VirtualDag::insert(CertifiedVertex)` is the existing API used by `produce_vertex_tick` at world.rs:271.)

- [ ] **Step 11: Build + test the workspace**

Run: `cargo check --workspace --locked`
Expected: clean.
Run: `cargo test -p net --locked && cargo test -p consensus --locked && cargo test -p sim --locked`
Expected: PASS, including the three new gossip_wire tests.

- [ ] **Step 12: Commit**

```bash
git add crates/consensus/src/event.rs crates/consensus/src/action.rs crates/consensus/src/state_machine.rs crates/net/src apps/sim/src/virtual_net.rs apps/sim/src/world.rs
git commit -m "feat(net,consensus,sim): vertex-proposal/partial wire plumbing end to end"
```

---

## Task 5: `PendingBlobSource` port on `HostContext`

Breaking change to `HostContext` — every construction site gets the new field in this one task.

**Files:**
- Create: `crates/consensus/src/ports/pending_blobs.rs`
- Modify: `crates/consensus/src/ports/mod.rs`, `crates/consensus/src/host_context.rs`
- Modify (construction sites): `crates/consensus/src/state_machine.rs` (test ctx), `crates/consensus/src/macro_fin/mod.rs` (test ctx ~line 902), `crates/consensus/tests/bullshark_happy.rs` (~234, ~266), `crates/consensus/tests/bullshark_commit.rs` (~214), `crates/consensus/tests/step_signature.rs` (~105), `apps/sim/src/world.rs` (~339), `apps/node/src/host_context.rs` (~145 + `StubHostBundle`), `apps/node/src/runtime.rs`, `apps/cli/src/stub_context.rs` (~113)

- [ ] **Step 1: Create the port**

`crates/consensus/src/ports/pending_blobs.rs`:

```rust
//! `PendingBlobSource` port: blobs queued on this node awaiting
//! inclusion in its next vertex proposal (06-04 design §5).

use types::dag::BlobRef;

/// Drains blob references pending local proposal inclusion.
///
/// `vertex_cert` calls [`PendingBlobSource::drain`] exactly once per
/// proposal it builds; drained refs ride in that vertex. Hosts without
/// blob custody plug in [`NoPendingBlobs`].
pub trait PendingBlobSource: Send + Sync {
    /// Pop every queued `BlobRef` in FIFO order.
    fn drain(&self) -> Vec<BlobRef>;
}

/// Stub for tests, sim, and hosts without blob custody.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoPendingBlobs;

impl PendingBlobSource for NoPendingBlobs {
    fn drain(&self) -> Vec<BlobRef> {
        Vec::new()
    }
}
```

- [ ] **Step 2: Wire into ports + `HostContext`**

`crates/consensus/src/ports/mod.rs` — add:

```rust
pub mod pending_blobs;
pub use pending_blobs::{NoPendingBlobs, PendingBlobSource};
```

`crates/consensus/src/host_context.rs` — extend the import and struct:

```rust
use crate::ports::{
    Clock, DagView, PendingBlobSource, Persistence, RandomnessBeacon, SignerPort,
    ValidatorSetPort,
};
```

and append the field to `HostContext`:

```rust
    /// Blobs queued locally for the next own-vertex proposal.
    pub pending_blobs: &'a dyn PendingBlobSource,
```

- [ ] **Step 3: Fix every construction site**

Run `cargo check --workspace --locked` and add the field to each `HostContext { ... }` literal the compiler reports. Known sites and the exact addition:

For consensus/cli/sim **test or stub** contexts (`state_machine.rs` test helper, `macro_fin/mod.rs` test ctx, `bullshark_happy.rs` ×2, `bullshark_commit.rs`, `step_signature.rs`, `apps/cli/src/stub_context.rs`):

```rust
    static NO_PENDING: consensus::ports::NoPendingBlobs = consensus::ports::NoPendingBlobs;
    // inside the literal:
        pending_blobs: &NO_PENDING,
```

(inside the `consensus` crate itself use `crate::ports::NoPendingBlobs`; non-static contexts may use a local `let no_pending = NoPendingBlobs;` and borrow it.)

`apps/sim/src/world.rs::step_validator` (~line 339) — sim has no blob plane:

```rust
        let no_pending = consensus::ports::NoPendingBlobs;
        let ctx = HostContext {
            dag: self.dag.as_ref(),
            clock: self.clock.as_ref(),
            valset: self.valset.as_ref(),
            beacon: self.beacon.as_ref(),
            persistence: self.persistence[idx].as_ref(),
            signer: &signer,
            pending_blobs: &no_pending,
        };
```

`apps/node/src/host_context.rs` — real adapter over custody:

```rust
use crate::blob::BlobCustodyHandle;
use consensus::ports::PendingBlobSource;
use types::dag::BlobRef;

/// `PendingBlobSource` over the node's blob custody queue.
///
/// `None` (custody disabled) drains nothing — proposals go out empty,
/// preserving liveness.
#[derive(Clone, Debug)]
pub struct CustodyPendingBlobs(Option<BlobCustodyHandle>);

impl CustodyPendingBlobs {
    /// Wrap an optional custody handle.
    #[must_use]
    pub fn new(custody: Option<BlobCustodyHandle>) -> Self {
        Self(custody)
    }
}

impl PendingBlobSource for CustodyPendingBlobs {
    fn drain(&self) -> Vec<BlobRef> {
        self.0.as_ref().map(BlobCustodyHandle::drain_pending).unwrap_or_default()
    }
}
```

(`BlobCustodyHandle` derives `Clone` but not `Debug`; if `#[derive(Debug)]` fails on `CustodyPendingBlobs`, write a manual `impl std::fmt::Debug` mirroring `ActionApplier`'s `finish_non_exhaustive()` pattern.)

`StubHostBundle` gains the port and its constructor a parameter:

```rust
pub struct StubHostBundle {
    pub dag: Arc<LiveDag>,
    pub clock: TokioClock,
    pub valset: CachedValidatorSet,
    pub beacon: Arc<ChainedBeacon>,
    pub signer: DevSigner,
    /// Pending-blob source for own-vertex proposals (06-04).
    pub pending_blobs: CustodyPendingBlobs,
}
```

`StubHostBundle::new(label, valset, dag, signer_key_path, blob_custody: Option<BlobCustodyHandle>)` — append the parameter, set `pending_blobs: CustodyPendingBlobs::new(blob_custody)`.

`build_host_context` — add `pending_blobs: &bundle.pending_blobs,` to the literal.

`apps/node/src/runtime.rs` — `StubHostBundle::new` is currently built **before** the swarm/custody block but needs the custody handle. Reorder: move the `let host_bundle = ...`, `let beacon = ...`, and `let action_applier = ...` statements to **after** the swarm `let (swarm_handle, net_ready_rx) = ...` block (nothing in that block reads them — verified), and pass the handle:

```rust
    let host_bundle = StubHostBundle::new(
        &cfg.node.identity.label,
        valset.clone(),
        Arc::clone(&live_dag),
        None,
        blob_custody_handle.clone(),
    )
    .context("build host context bundle")?;
```

Other `StubHostBundle::new` call sites: `rg "StubHostBundle::new"` — append `, None` as the fifth argument in every test (`apps/node/tests/l1_driver_smoke.rs` and any others the grep finds).

- [ ] **Step 4: Workspace check + full test**

Run: `cargo test --workspace --locked`
Expected: all green (behavior unchanged — only stubs added).

- [ ] **Step 5: Commit**

```bash
git add crates/consensus apps/sim/src/world.rs apps/node/src apps/node/tests apps/cli/src/stub_context.rs
git commit -m "feat(consensus): PendingBlobSource port on HostContext; node custody adapter"
```

---

## Task 6: `vertex_cert` module skeleton — `VertexBook` + `verify`

**Files:**
- Create: `crates/consensus/src/vertex_cert/mod.rs`, `crates/consensus/src/vertex_cert/book.rs`, `crates/consensus/src/vertex_cert/verify.rs`
- Modify: `crates/consensus/src/lib.rs` (`pub mod vertex_cert;` in the module list)

- [ ] **Step 1: `book.rs`**

```rust
//! Per-validator distributed vertex-certification state (06-04 design §2).

use std::collections::{BTreeMap, HashMap, HashSet};

use types::{
    crypto_types::{BlsSig, Hash32},
    dag::{CertifiedVertex, VertexProposal},
    primitives::{Round, ValidatorId},
};

use crate::{event::TimerId, leader::timeout::TimerScheduler};

/// Per-validator vertex-certification state held by `StateMachine`.
#[derive(Debug)]
pub struct VertexBook {
    /// Validator this book belongs to (`author = self` on own proposals).
    pub(crate) self_id: ValidatorId,
    /// Round of this node's latest own proposal.
    pub(crate) current_round: Round,
    /// `genesis_propose` already ran (idempotence guard).
    pub(crate) started: bool,
    /// Certified vertices seen, by round: author → vertex hash.
    /// `BTreeMap` keys give the deterministic author order for parents.
    pub(crate) certified_by_round: BTreeMap<Round, BTreeMap<ValidatorId, Hash32>>,
    /// This node's own proposals being collected, by vertex hash.
    pub(crate) my_proposals: HashMap<Hash32, VertexProposal>,
    /// Partials collected for own proposals: vertex hash → voter → sig.
    pub(crate) collecting: HashMap<Hash32, BTreeMap<ValidatorId, BlsSig>>,
    /// Hashes for which `BroadcastCertifiedVertex` was already emitted.
    pub(crate) emitted_certs: HashSet<Hash32>,
    /// Proposals seen per `(round, author)` → equivocation detection.
    pub(crate) proposals_seen: HashMap<(Round, ValidatorId), Vec<VertexProposal>>,
    /// `(round, author)` pairs this node already voted for.
    pub(crate) voted: HashSet<(Round, ValidatorId)>,
    /// Active fallback timer for `current_round`.
    pub(crate) round_timer: Option<TimerId>,
    /// Consecutive timer fires without round progress (linear backoff).
    pub(crate) timer_retries: u32,
    /// Monotonic timer-id allocator (separate namespace per book).
    pub(crate) timers: TimerScheduler,
    /// Invalid crypto dropped on receive (proposals + partials).
    pub(crate) rejected_crypto: u64,
    /// A voter sent two different sigs for the same vertex (kept first).
    pub(crate) partial_conflicts: u64,
    /// Round-timer fires while the round still lacked `2f+1` certs.
    pub(crate) rounds_stalled: u64,
}

impl VertexBook {
    /// Fresh book for `self_id` at round 0, not yet started.
    #[must_use]
    pub fn new(self_id: ValidatorId) -> Self {
        Self {
            self_id,
            current_round: Round(0),
            started: false,
            certified_by_round: BTreeMap::new(),
            my_proposals: HashMap::new(),
            collecting: HashMap::new(),
            emitted_certs: HashSet::new(),
            proposals_seen: HashMap::new(),
            voted: HashSet::new(),
            round_timer: None,
            timer_retries: 0,
            timers: TimerScheduler::default(),
            rejected_crypto: 0,
            partial_conflicts: 0,
            rounds_stalled: 0,
        }
    }

    /// Round of this node's latest own proposal (sim/test probe).
    #[must_use]
    pub fn current_round(&self) -> u64 {
        self.current_round.0
    }

    /// Test helper: invalid crypto drops.
    #[must_use]
    pub fn rejected_crypto(&self) -> u64 {
        self.rejected_crypto
    }

    /// Test helper: conflicting duplicate partials.
    #[must_use]
    pub fn partial_conflicts(&self) -> u64 {
        self.partial_conflicts
    }

    /// Test helper: stalled-round timer fires.
    #[must_use]
    pub fn rounds_stalled(&self) -> u64 {
        self.rounds_stalled
    }

    /// Number of certified vertices known at `round` (sim/test probe).
    #[must_use]
    pub fn certified_count_at(&self, round: Round) -> usize {
        self.certified_by_round.get(&round).map_or(0, BTreeMap::len)
    }
}
```

- [ ] **Step 2: `verify.rs`**

```rust
//! Pure signature checks for vertex proposals and partials.

use crypto::{bls::PublicKey, bls::sign::verify, hash::dst};
use types::{
    dag::{Vertex, VertexPartial, VertexProposal},
    primitives::ValidatorId,
    validator::{ValidatorEntry, ValidatorSet},
};

fn entry_for<'a>(set: &'a ValidatorSet, id: &ValidatorId) -> Option<&'a ValidatorEntry> {
    set.entries.iter().find(|e| &e.id == id)
}

fn pk_for(set: &ValidatorSet, id: &ValidatorId) -> Option<PublicKey> {
    PublicKey::from_bytes(&entry_for(set, id)?.bls_pubkey).ok()
}

/// Verify a proposal's authority signature: author ∈ valset and
/// `proposer_sig` valid under [`dst::VERTEX_PROPOSAL`] over
/// `signing_bytes(vertex)`. Hash integrity is checked by the caller.
#[must_use]
pub fn verify_proposal(set: &ValidatorSet, p: &VertexProposal) -> bool {
    let Some(pk) = pk_for(set, &p.vertex.author) else {
        return false;
    };
    let msg = dag::signing::signing_bytes(&p.vertex);
    verify(&pk, dst::VERTEX_PROPOSAL, &msg, &p.proposer_sig).is_ok()
}

/// Verify a partial vote against the proposal's vertex: routing fields
/// match, voter ∈ valset, and `sig` valid under [`dst::VERTEX_CERT`]
/// over `signing_bytes(vertex)`.
#[must_use]
pub fn verify_partial(set: &ValidatorSet, bp: &VertexPartial, vertex: &Vertex) -> bool {
    if bp.vertex_hash != vertex.hash || bp.round != vertex.round || bp.author != vertex.author
    {
        return false;
    }
    let Some(pk) = pk_for(set, &bp.voter) else {
        return false;
    };
    let msg = dag::signing::signing_bytes(vertex);
    verify(&pk, dst::VERTEX_CERT, &msg, &bp.sig).is_ok()
}
```

- [ ] **Step 3: `mod.rs` skeleton + shared test fixture**

`crates/consensus/src/vertex_cert/mod.rs` — module shell; handlers land in Tasks 7–9, but the **test fixture** used by all of them is defined now:

```rust
//! L1 distributed vertex certification (06-04 design).
//!
//! Mirrors `macro_fin`: deterministic handlers over a per-validator
//! [`VertexBook`], driven by `StateMachine::step`. The proposer (only)
//! aggregates partials for its own vertex; rounds advance cert-driven
//! (Narwhal) with a re-broadcast fallback timer — never a round jump.

pub mod book;
pub mod verify;

pub use book::VertexBook;

use types::{
    crypto_types::Hash32,
    dag::{CertifiedVertex, Vertex, VertexPartial, VertexProposal},
    primitives::{Epoch, Round},
    validator::ValidatorSet,
};

use crypto::hash::dst;

use crate::{
    action::Action,
    config::Config,
    error::Result,
    event::TimerId,
    host_context::HostContext,
    state_machine::Actions,
};

fn active_set(ctx: &HostContext<'_>) -> Result<ValidatorSet> {
    ctx.valset
        .set_for(Epoch(0))?
        .ok_or_else(|| crate::Error::InvalidConfig("no validator set for epoch 0".into()))
}

fn quorum_need(set: &ValidatorSet) -> Result<usize> {
    let n = u32::try_from(set.entries.len())
        .map_err(|_| crate::Error::InvalidConfig("validator set too large".into()))?;
    Ok(dag::cert::quorum_threshold(n) as usize)
}

fn index_of(set: &ValidatorSet, id: &types::primitives::ValidatorId) -> Option<u32> {
    set.entries
        .iter()
        .position(|e| &e.id == id)
        .map(|i| u32::try_from(i).unwrap_or(u32::MAX))
}

/// Accept messages only for rounds near our own (anti-spam memory bound).
fn round_in_window(book: &VertexBook, round: Round) -> bool {
    round.0 + 1 >= book.current_round.0 && round.0 <= book.current_round.0 + 1
}

#[cfg(test)]
pub(crate) mod test_fixture {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crypto::bls::keys::SecretKey;
    use types::{
        crypto_types::{BlsSig, Hash32, VrfPubkey, VrfProof},
        primitives::{Epoch, StakeWeight, ValidatorId},
        validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
    };

    use crate::ports::{
        Clock, DagView, NoPendingBlobs, Persistence, RandomnessBeacon, SignerPort,
        ValidatorSetPort,
    };

    pub struct Ring {
        pub sks: Vec<SecretKey>,
        pub set: ValidatorSet,
    }

    impl Ring {
        pub fn new(n: u8) -> Self {
            let sks: Vec<SecretKey> = (0..n)
                .map(|i| SecretKey::from_ikm(&[i + 1; 32]).unwrap())
                .collect();
            let entries = (0..n)
                .map(|i| ValidatorEntry {
                    id: ValidatorId([i; 32]),
                    bls_pubkey: sks[i as usize].public().to_bytes(),
                    vrf_pubkey: VrfPubkey::zero(),
                    stake: StakeWeight(1),
                    identity: ValidatorIdentity {
                        asn: None,
                        cloud: None,
                        region: None,
                    },
                })
                .collect();
            let set = ValidatorSet {
                epoch: Epoch(0),
                entries,
                total_stake: StakeWeight(u64::from(n)),
            };
            Self { sks, set }
        }

        pub fn id(&self, i: u8) -> ValidatorId {
            ValidatorId([i; 32])
        }

        pub fn sign(&self, i: u8, dst: &[u8], msg: &[u8]) -> BlsSig {
            crypto::bls::sign::sign(&self.sks[i as usize], dst, msg)
        }
    }

    pub struct RingSigner<'a> {
        pub ring: &'a Ring,
        pub idx: usize,
    }

    impl SignerPort for RingSigner<'_> {
        fn sign_bls(&self, dst: &[u8], msg: &[u8]) -> BlsSig {
            crypto::bls::sign::sign(&self.ring.sks[self.idx], dst, msg)
        }
        fn vrf_prove(&self, _alpha: &[u8]) -> crate::error::Result<(VrfProof, Hash32)> {
            Ok((VrfProof::zero(), Hash32::zero()))
        }
    }

    pub struct FixedValset(pub ValidatorSet);
    impl ValidatorSetPort for FixedValset {
        fn set_for(&self, epoch: Epoch) -> crate::error::Result<Option<ValidatorSet>> {
            Ok((self.0.epoch == epoch).then(|| self.0.clone()))
        }
        fn index_of(
            &self,
            _epoch: Epoch,
            v: &ValidatorId,
        ) -> crate::error::Result<Option<u32>> {
            Ok(self
                .0
                .entries
                .iter()
                .position(|e| &e.id == v)
                .map(|i| u32::try_from(i).unwrap()))
        }
    }

    pub struct EmptyDag;
    impl DagView for EmptyDag {
        fn vertex(
            &self,
            _h: &Hash32,
        ) -> crate::error::Result<Option<types::dag::CertifiedVertex>> {
            Ok(None)
        }
        fn vertices_at_round(
            &self,
            _r: types::primitives::Round,
        ) -> crate::error::Result<Vec<types::dag::CertifiedVertex>> {
            Ok(vec![])
        }
    }

    pub struct ZeroClock;
    impl Clock for ZeroClock {
        fn now_nanos(&self) -> u128 {
            0
        }
    }

    pub struct ZeroBeacon;
    impl RandomnessBeacon for ZeroBeacon {
        fn current(&self) -> crate::error::Result<Hash32> {
            Ok(Hash32::zero())
        }
    }

    #[derive(Default)]
    pub struct MemPersistence(pub Mutex<HashMap<u8, u8>>);
    impl Persistence for MemPersistence {
        fn store_micro_qc(&self, _qc: &types::micro::MicroQc) -> crate::error::Result<()> {
            Ok(())
        }
        fn micro_qc_for(
            &self,
            _h: &Hash32,
        ) -> crate::error::Result<Option<types::micro::MicroQc>> {
            Ok(None)
        }
        fn store_macro_checkpoint(
            &self,
            _cp: &types::macros::MacroCheckpoint,
        ) -> crate::error::Result<()> {
            Ok(())
        }
        fn store_macro_qc(&self, _qc: &types::macros::MacroQc) -> crate::error::Result<()> {
            Ok(())
        }
        fn append_slash_evidence(
            &self,
            _ev: &types::slashing::SlashEvidence,
        ) -> crate::error::Result<()> {
            Ok(())
        }
        fn macro_checkpoint_at(
            &self,
            _h: types::primitives::Height,
        ) -> crate::error::Result<Option<types::macros::MacroCheckpoint>> {
            Ok(None)
        }
        fn macro_qc_for(
            &self,
            _h: &Hash32,
        ) -> crate::error::Result<Option<types::macros::MacroQc>> {
            Ok(None)
        }
    }

    /// Build a `HostContext` over fixture ports for validator `idx`.
    /// Returns the owned ports; destructure and borrow at the call site:
    ///
    /// ```ignore
    /// let ring = Ring::new(4);
    /// let valset = FixedValset(ring.set.clone());
    /// let signer = RingSigner { ring: &ring, idx: 0 };
    /// let (dag, clock, beacon, persist, no_pending) =
    ///     (EmptyDag, ZeroClock, ZeroBeacon, MemPersistence::default(), NoPendingBlobs);
    /// let ctx = HostContext {
    ///     dag: &dag, clock: &clock, valset: &valset, beacon: &beacon,
    ///     persistence: &persist, signer: &signer, pending_blobs: &no_pending,
    /// };
    /// ```
    pub fn _doc_only() {}
}
```

Add to `crates/consensus/src/lib.rs` module list (alphabetical):

```rust
pub mod vertex_cert;
```

- [ ] **Step 4: Verify-layer unit tests**

Append to `verify.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::vertex_cert::test_fixture::Ring;
    use crypto::hash::dst;
    use types::{crypto_types::Hash32, primitives::Round};

    fn sealed(ring: &Ring, author: u8) -> Vertex {
        let mut v = Vertex {
            round: Round(0),
            author: ring.id(author),
            parents: vec![],
            blobs: vec![],
            hash: Hash32::zero(),
        };
        dag::signing::seal_hash(&mut v);
        v
    }

    #[test]
    fn proposal_sig_verifies_and_forgery_fails() {
        let ring = Ring::new(4);
        let vertex = sealed(&ring, 0);
        let msg = dag::signing::signing_bytes(&vertex);
        let good = VertexProposal {
            vertex: vertex.clone(),
            proposer_sig: ring.sign(0, dst::VERTEX_PROPOSAL, &msg),
        };
        assert!(verify_proposal(&ring.set, &good));
        // signed by the wrong validator
        let forged = VertexProposal {
            vertex,
            proposer_sig: ring.sign(1, dst::VERTEX_PROPOSAL, &msg),
        };
        assert!(!verify_proposal(&ring.set, &forged));
    }

    #[test]
    fn partial_verifies_and_field_mismatch_fails() {
        let ring = Ring::new(4);
        let vertex = sealed(&ring, 0);
        let msg = dag::signing::signing_bytes(&vertex);
        let good = VertexPartial {
            vertex_hash: vertex.hash,
            round: vertex.round,
            author: vertex.author,
            voter: ring.id(1),
            sig: ring.sign(1, dst::VERTEX_CERT, &msg),
        };
        assert!(verify_partial(&ring.set, &good, &vertex));
        let mut wrong_round = good.clone();
        wrong_round.round = Round(9);
        assert!(!verify_partial(&ring.set, &wrong_round, &vertex));
        let mut unknown_voter = good;
        unknown_voter.voter = types::primitives::ValidatorId([0xEE; 32]);
        assert!(!verify_partial(&ring.set, &unknown_voter, &vertex));
    }
}
```

- [ ] **Step 5: Run**

Run: `cargo test -p consensus vertex_cert --locked`
Expected: 2 verify tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/consensus/src/vertex_cert crates/consensus/src/lib.rs
git commit -m "feat(consensus): vertex_cert module skeleton — VertexBook and verify"
```

---

## Task 7: `on_vertex_proposal`

**Files:**
- Modify: `crates/consensus/src/vertex_cert/mod.rs`

- [ ] **Step 1: Write the failing tests** (append a `#[cfg(test)] mod proposal_tests` to `mod.rs`)

```rust
#[cfg(test)]
mod proposal_tests {
    use super::test_fixture::*;
    use super::*;
    use crate::ports::NoPendingBlobs;
    use types::primitives::ValidatorId;

    fn ctx_parts() -> (EmptyDag, ZeroClock, ZeroBeacon, MemPersistence, NoPendingBlobs) {
        (
            EmptyDag,
            ZeroClock,
            ZeroBeacon,
            MemPersistence::default(),
            NoPendingBlobs,
        )
    }

    fn proposal_from(ring: &Ring, author: u8, round: u64, parents: Vec<Hash32>) -> VertexProposal {
        let mut vertex = Vertex {
            round: Round(round),
            author: ring.id(author),
            parents,
            blobs: vec![],
            hash: Hash32::zero(),
        };
        dag::signing::seal_hash(&mut vertex);
        let sig = ring.sign(
            author,
            dst::VERTEX_PROPOSAL,
            &dag::signing::signing_bytes(&vertex),
        );
        VertexProposal {
            vertex,
            proposer_sig: sig,
        }
    }

    #[test]
    fn valid_genesis_proposal_yields_exactly_one_partial() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 1 };
        let (dag_p, clock, beacon, persist, no_pending) = ctx_parts();
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(1));
        let cfg = Config::default_table_17_1();
        let p = proposal_from(&ring, 0, 0, vec![]);
        let actions = on_vertex_proposal(&mut book, &cfg, p.clone(), &ctx).unwrap();
        assert_eq!(actions.len(), 1);
        let Action::BroadcastVertexPartial(bp) = &actions[0] else {
            panic!("expected partial, got {actions:?}");
        };
        assert_eq!(bp.vertex_hash, p.vertex.hash);
        assert_eq!(bp.author, ring.id(0));
        assert_eq!(bp.voter, ring.id(1));
        assert!(verify::verify_partial(&ring.set, bp, &p.vertex));
        // re-delivery of the same proposal: no second vote
        let again = on_vertex_proposal(&mut book, &cfg, p, &ctx).unwrap();
        assert!(again.is_empty());
    }

    #[test]
    fn bad_proposer_sig_is_dropped() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 1 };
        let (dag_p, clock, beacon, persist, no_pending) = ctx_parts();
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(1));
        let cfg = Config::default_table_17_1();
        let mut p = proposal_from(&ring, 0, 0, vec![]);
        p.proposer_sig = types::crypto_types::BlsSig([0xEE; 96]);
        let actions = on_vertex_proposal(&mut book, &cfg, p, &ctx).unwrap();
        assert!(actions.is_empty());
        assert_eq!(book.rejected_crypto(), 1);
    }

    #[test]
    fn uncertified_parents_hold_then_vote_on_redelivery() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 1 };
        let (dag_p, clock, beacon, persist, no_pending) = ctx_parts();
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(1));
        let cfg = Config::default_table_17_1();
        let parents = vec![Hash32([1; 32]), Hash32([2; 32]), Hash32([3; 32])];
        let p = proposal_from(&ring, 0, 1, parents.clone());
        // round-1 certs unknown → hold, no vote
        let held = on_vertex_proposal(&mut book, &cfg, p.clone(), &ctx).unwrap();
        assert!(held.is_empty());
        // certs for round 0 arrive (3 distinct authors matching the parents)
        for (i, h) in parents.iter().enumerate() {
            book.certified_by_round
                .entry(Round(0))
                .or_default()
                .insert(ring.id(u8::try_from(i).unwrap()), *h);
        }
        // re-delivery (proposer re-broadcast) now votes
        let actions = on_vertex_proposal(&mut book, &cfg, p, &ctx).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::BroadcastVertexPartial(_)));
    }

    #[test]
    fn double_propose_different_hash_emits_slash_evidence() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 1 };
        let (dag_p, clock, beacon, persist, no_pending) = ctx_parts();
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(1));
        let cfg = Config::default_table_17_1();
        let a = proposal_from(&ring, 0, 0, vec![]);
        let b = proposal_from(&ring, 0, 0, vec![Hash32([9; 32])]); // different content
        assert_ne!(a.vertex.hash, b.vertex.hash);
        let _ = on_vertex_proposal(&mut book, &cfg, a, &ctx).unwrap();
        let actions = on_vertex_proposal(&mut book, &cfg, b, &ctx).unwrap();
        assert_eq!(actions.len(), 1);
        let Action::EmitSlashEvidence { offender, evidence } = &actions[0] else {
            panic!("expected slash evidence, got {actions:?}");
        };
        assert_eq!(*offender, ring.id(0));
        let types::slashing::SlashEvidence::VertexEquivocation(ev) = evidence else {
            panic!("expected VertexEquivocation");
        };
        crate::slashing::vertex_equivocation::verify(ev, &ring.set)
            .expect("emitted evidence must verify offline");
    }

    #[test]
    fn out_of_window_round_is_dropped() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 1 };
        let (dag_p, clock, beacon, persist, no_pending) = ctx_parts();
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(1)); // current_round = 0
        let cfg = Config::default_table_17_1();
        let far_future = proposal_from(&ring, 0, 50, vec![Hash32([1; 32])]);
        let actions = on_vertex_proposal(&mut book, &cfg, far_future, &ctx).unwrap();
        assert!(actions.is_empty());
        assert!(book.proposals_seen.is_empty(), "must not buffer far-future rounds");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p consensus vertex_cert --locked`
Expected: compile FAIL — `on_vertex_proposal` not found.

- [ ] **Step 3: Implement the handler in `mod.rs`**

```rust
/// Handle an inbound vertex proposal: verify, detect equivocation,
/// require certified parents, then vote (one partial per `(round, author)`).
pub fn on_vertex_proposal(
    book: &mut VertexBook,
    _cfg: &Config,
    p: VertexProposal,
    ctx: &HostContext<'_>,
) -> Result<Actions> {
    let mut actions = Actions::new();
    let set = active_set(ctx)?;

    // 1. Integrity + authority. Hash check first — everything signs the body.
    if p.vertex.hash != dag::signing::content_hash(&p.vertex) {
        book.rejected_crypto += 1;
        return Ok(actions);
    }
    if !verify::verify_proposal(&set, &p) {
        book.rejected_crypto += 1;
        return Ok(actions);
    }

    let round = p.vertex.round;
    let author = p.vertex.author;
    if author == book.self_id {
        // Own proposal echoed back via gossip — already seeded locally.
        return Ok(actions);
    }

    // 2. Memory bound: only rounds near our own.
    if !round_in_window(book, round) {
        return Ok(actions);
    }

    // 3. Equivocation: same (round, author), different hash → evidence.
    //    Same hash → known re-broadcast; fall through so a held proposal
    //    can still be voted once its parents certify.
    let key = (round, author);
    match book.proposals_seen.get(&key) {
        Some(existing)
            if existing.iter().any(|prev| prev.vertex.hash != p.vertex.hash) =>
        {
            let first = existing[0].clone();
            actions.push(Action::EmitSlashEvidence {
                offender: author,
                evidence: types::slashing::SlashEvidence::VertexEquivocation(
                    crate::slashing::vertex_equivocation::detect(author, first, p),
                ),
            });
            return Ok(actions);
        }
        Some(_) => {} // identical re-broadcast: already recorded
        None => book.proposals_seen.entry(key).or_default().push(p.clone()),
    }

    // 4. Parents must be ≥2f+1 certified vertices we know at round-1
    //    (genesis round 0 is exempt: parents = []).
    if round.0 > 0 {
        let need = quorum_need(&set)?;
        let prev = book.certified_by_round.get(&Round(round.0 - 1));
        let known = |h: &Hash32| prev.is_some_and(|m| m.values().any(|ph| ph == h));
        if p.vertex.parents.len() < need || !p.vertex.parents.iter().all(known) {
            return Ok(actions); // hold — re-broadcast will retry the vote
        }
    } else if !p.vertex.parents.is_empty() {
        book.rejected_crypto += 1;
        return Ok(actions);
    }

    // 5. One vote per (round, author).
    if book.voted.contains(&key) {
        return Ok(actions);
    }
    book.voted.insert(key);
    let sig = ctx
        .signer
        .sign_bls(dst::VERTEX_CERT, &dag::signing::signing_bytes(&p.vertex));
    actions.push(Action::BroadcastVertexPartial(VertexPartial {
        vertex_hash: p.vertex.hash,
        round,
        author,
        voter: book.self_id,
        sig,
    }));
    Ok(actions)
}
```

- [ ] **Step 4: Run**

Run: `cargo test -p consensus vertex_cert --locked`
Expected: all 5 proposal tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/consensus/src/vertex_cert/mod.rs
git commit -m "feat(consensus): vertex_cert::on_vertex_proposal with equivocation + hold"
```

---

## Task 8: `on_vertex_partial` + `try_finalize`

**Files:**
- Modify: `crates/consensus/src/vertex_cert/mod.rs`

- [ ] **Step 1: Write the failing tests** (append `mod partial_tests`)

```rust
#[cfg(test)]
mod partial_tests {
    use super::test_fixture::*;
    use super::*;
    use crate::ports::NoPendingBlobs;

    /// Seed `book` with an own proposal at `round` exactly as
    /// `propose_round` (Task 9) will: my_proposals + self-partial + voted.
    fn seed_own_proposal(book: &mut VertexBook, ring: &Ring, idx: u8, round: u64) -> VertexProposal {
        let mut vertex = Vertex {
            round: Round(round),
            author: ring.id(idx),
            parents: vec![],
            blobs: vec![],
            hash: Hash32::zero(),
        };
        dag::signing::seal_hash(&mut vertex);
        let msg = dag::signing::signing_bytes(&vertex);
        let proposal = VertexProposal {
            vertex: vertex.clone(),
            proposer_sig: ring.sign(idx, dst::VERTEX_PROPOSAL, &msg),
        };
        book.my_proposals.insert(vertex.hash, proposal.clone());
        book.collecting
            .entry(vertex.hash)
            .or_default()
            .insert(ring.id(idx), ring.sign(idx, dst::VERTEX_CERT, &msg));
        book.voted.insert((Round(round), ring.id(idx)));
        book.current_round = Round(round);
        proposal
    }

    fn partial(ring: &Ring, voter: u8, p: &VertexProposal) -> VertexPartial {
        VertexPartial {
            vertex_hash: p.vertex.hash,
            round: p.vertex.round,
            author: p.vertex.author,
            voter: ring.id(voter),
            sig: ring.sign(
                voter,
                dst::VERTEX_CERT,
                &dag::signing::signing_bytes(&p.vertex),
            ),
        }
    }

    #[test]
    fn quorum_partials_emit_verifying_certified_vertex() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 0 };
        let (dag_p, clock, beacon, persist, no_pending) = (
            EmptyDag,
            ZeroClock,
            ZeroBeacon,
            MemPersistence::default(),
            NoPendingBlobs,
        );
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(0));
        let cfg = Config::default_table_17_1();
        let p = seed_own_proposal(&mut book, &ring, 0, 0);

        // partial #2 (self + 1 = 2 < 3): below quorum, no cert yet
        let a1 = on_vertex_partial(&mut book, &cfg, partial(&ring, 1, &p), &ctx).unwrap();
        assert!(a1.is_empty());
        // partial #3 reaches 2f+1 = 3 → cert
        let a2 = on_vertex_partial(&mut book, &cfg, partial(&ring, 2, &p), &ctx).unwrap();
        let cv = a2
            .iter()
            .find_map(|a| match a {
                Action::BroadcastCertifiedVertex(cv) => Some(cv.clone()),
                _ => None,
            })
            .expect("cert emitted at quorum");
        dag::cert::verify_certified_vertex(&cv, &ring.set).expect("cert must verify");
        assert_eq!(book.certified_count_at(Round(0)), 1, "self-recorded");
        // late 4th partial: cert already emitted, no duplicate
        let a3 = on_vertex_partial(&mut book, &cfg, partial(&ring, 3, &p), &ctx).unwrap();
        assert!(!a3
            .iter()
            .any(|a| matches!(a, Action::BroadcastCertifiedVertex(_))));
    }

    #[test]
    fn forged_partial_is_dropped() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 0 };
        let (dag_p, clock, beacon, persist, no_pending) = (
            EmptyDag,
            ZeroClock,
            ZeroBeacon,
            MemPersistence::default(),
            NoPendingBlobs,
        );
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(0));
        let cfg = Config::default_table_17_1();
        let p = seed_own_proposal(&mut book, &ring, 0, 0);
        let mut bad = partial(&ring, 1, &p);
        bad.sig = types::crypto_types::BlsSig([0xEE; 96]);
        let actions = on_vertex_partial(&mut book, &cfg, bad, &ctx).unwrap();
        assert!(actions.is_empty());
        assert_eq!(book.rejected_crypto(), 1);
        assert_eq!(book.collecting[&p.vertex.hash].len(), 1, "only self sig");
    }

    #[test]
    fn partial_for_foreign_author_is_ignored() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 0 };
        let (dag_p, clock, beacon, persist, no_pending) = (
            EmptyDag,
            ZeroClock,
            ZeroBeacon,
            MemPersistence::default(),
            NoPendingBlobs,
        );
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        // book belongs to validator 3; partial addressed to author 0
        let mut book = VertexBook::new(ring.id(3));
        let cfg = Config::default_table_17_1();
        let mut other = VertexBook::new(ring.id(0));
        let p = seed_own_proposal(&mut other, &ring, 0, 0);
        let actions = on_vertex_partial(&mut book, &cfg, partial(&ring, 1, &p), &ctx).unwrap();
        assert!(actions.is_empty());
        assert!(book.collecting.is_empty());
    }

    #[test]
    fn duplicate_voter_counts_once_and_conflict_keeps_first() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 0 };
        let (dag_p, clock, beacon, persist, no_pending) = (
            EmptyDag,
            ZeroClock,
            ZeroBeacon,
            MemPersistence::default(),
            NoPendingBlobs,
        );
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(0));
        let cfg = Config::default_table_17_1();
        let p = seed_own_proposal(&mut book, &ring, 0, 0);
        let bp = partial(&ring, 1, &p);
        let _ = on_vertex_partial(&mut book, &cfg, bp.clone(), &ctx).unwrap();
        let _ = on_vertex_partial(&mut book, &cfg, bp.clone(), &ctx).unwrap();
        assert_eq!(book.collecting[&p.vertex.hash].len(), 2); // self + voter 1
        assert_eq!(book.partial_conflicts(), 0);
        // same voter, different (still valid-format) sig bytes → conflict metric
        let mut conflicting = bp;
        conflicting.sig = ring.sign(1, dst::VERTEX_CERT, b"different message");
        let _ = on_vertex_partial(&mut book, &cfg, conflicting, &ctx).unwrap();
        assert_eq!(book.partial_conflicts(), 1);
        assert_eq!(book.collecting[&p.vertex.hash].len(), 2, "first kept");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p consensus vertex_cert --locked` → compile FAIL (`on_vertex_partial` missing).

- [ ] **Step 3: Implement in `mod.rs`**

```rust
/// Proposer-side vote collection for this node's own proposals.
pub fn on_vertex_partial(
    book: &mut VertexBook,
    cfg: &Config,
    bp: VertexPartial,
    ctx: &HostContext<'_>,
) -> Result<Actions> {
    let mut actions = Actions::new();
    // Only the proposer aggregates its own vertex.
    if bp.author != book.self_id || bp.voter == book.self_id {
        return Ok(actions);
    }
    let Some(proposal) = book.my_proposals.get(&bp.vertex_hash).cloned() else {
        return Ok(actions);
    };
    let set = active_set(ctx)?;
    if !verify::verify_partial(&set, &bp, &proposal.vertex) {
        book.rejected_crypto += 1;
        return Ok(actions);
    }
    let slot = book.collecting.entry(bp.vertex_hash).or_default();
    if let Some(prev) = slot.get(&bp.voter) {
        if *prev != bp.sig {
            // Voter equivocation on partials: drop + metric (slashing deferred).
            book.partial_conflicts += 1;
        }
        return Ok(actions);
    }
    slot.insert(bp.voter, bp.sig);
    try_finalize(book, cfg, ctx, bp.vertex_hash, &mut actions)?;
    Ok(actions)
}

/// Emit `BroadcastCertifiedVertex` once `collecting[hash]` reaches 2f+1.
/// Idempotent per hash. Also self-records the cert and tries to advance.
fn try_finalize(
    book: &mut VertexBook,
    cfg: &Config,
    ctx: &HostContext<'_>,
    hash: Hash32,
    actions: &mut Actions,
) -> Result<()> {
    if book.emitted_certs.contains(&hash) {
        return Ok(());
    }
    let Some(proposal) = book.my_proposals.get(&hash).cloned() else {
        return Ok(());
    };
    let set = active_set(ctx)?;
    let need = quorum_need(&set)?;
    let Some(sigs) = book.collecting.get(&hash) else {
        return Ok(());
    };
    if sigs.len() < need {
        return Ok(());
    }
    let mut contributors = Vec::with_capacity(sigs.len());
    for (voter, sig) in sigs {
        let Some(idx) = index_of(&set, voter) else {
            continue; // verified on receipt; unknown here only on epoch change
        };
        contributors.push((idx, *sig));
    }
    let cv = dag::cert::assemble_cert(&proposal.vertex, &set, &contributors)
        .map_err(|e| crate::Error::InvalidConfig(format!("assemble vertex cert: {e}")))?;
    dag::cert::verify_certified_vertex(&cv, &set)
        .map_err(|e| crate::Error::InvalidConfig(format!("self-check vertex cert: {e}")))?;
    book.emitted_certs.insert(hash);
    record_cert(book, &cv);
    actions.push(Action::BroadcastCertifiedVertex(cv));
    maybe_advance(book, cfg, ctx, actions)?;
    Ok(())
}

/// Record a certified vertex (idempotent by `(round, author)`).
fn record_cert(book: &mut VertexBook, cv: &CertifiedVertex) {
    book.certified_by_round
        .entry(cv.vertex.round)
        .or_default()
        .insert(cv.vertex.author, cv.vertex.hash);
}
```

For this task only, add a stub so the module compiles before Task 9 implements the real advancement:

```rust
/// Cert-driven round advancement (implemented in Task 9).
fn maybe_advance(
    _book: &mut VertexBook,
    _cfg: &Config,
    _ctx: &HostContext<'_>,
    _actions: &mut Actions,
) -> Result<()> {
    Ok(())
}
```

- [ ] **Step 4: Run**

Run: `cargo test -p consensus vertex_cert --locked`
Expected: all partial tests PASS (the quorum test asserts cert emission + verification, which works without advancement).

- [ ] **Step 5: Commit**

```bash
git add crates/consensus/src/vertex_cert/mod.rs
git commit -m "feat(consensus): vertex_cert proposer-side partial collection and cert assembly"
```

---

## Task 9: `propose_round`, `maybe_advance`, `genesis_propose`, `on_certified_vertex`, `on_timer_fired`

**Files:**
- Modify: `crates/consensus/src/vertex_cert/mod.rs`

- [ ] **Step 1: Write the failing tests** (append `mod advance_tests`)

```rust
#[cfg(test)]
mod advance_tests {
    use super::test_fixture::*;
    use super::*;
    use crate::ports::NoPendingBlobs;

    struct Fix {
        ring: Ring,
    }

    impl Fix {
        fn new() -> Self {
            Self { ring: Ring::new(4) }
        }
    }

    macro_rules! ctx {
        ($fix:ident, $signer_idx:expr, $valset:ident, $signer:ident, $parts:ident) => {
            let $valset = FixedValset($fix.ring.set.clone());
            let $signer = RingSigner {
                ring: &$fix.ring,
                idx: $signer_idx,
            };
            let $parts = (
                EmptyDag,
                ZeroClock,
                ZeroBeacon,
                MemPersistence::default(),
                NoPendingBlobs,
            );
            let ctx = HostContext {
                dag: &$parts.0,
                clock: &$parts.1,
                valset: &$valset,
                beacon: &$parts.2,
                persistence: &$parts.3,
                signer: &$signer,
                pending_blobs: &$parts.4,
            };
        };
    }

    fn cert_for(ring: &Ring, author: u8, round: u64) -> CertifiedVertex {
        let mut vertex = Vertex {
            round: Round(round),
            author: ring.id(author),
            parents: vec![],
            blobs: vec![],
            hash: Hash32::zero(),
        };
        dag::signing::seal_hash(&mut vertex);
        let msg = dag::signing::signing_bytes(&vertex);
        let contributors: Vec<_> = (0u8..3)
            .map(|i| (u32::from(i), ring.sign(i, dst::VERTEX_CERT, &msg)))
            .collect();
        dag::cert::assemble_cert(&vertex, &ring.set, &contributors).unwrap()
    }

    #[test]
    fn genesis_propose_emits_proposal_and_timer_and_is_idempotent() {
        let fix = Fix::new();
        ctx!(fix, 0, valset, signer, parts);
        let mut book = VertexBook::new(fix.ring.id(0));
        let cfg = Config::default_table_17_1();
        let actions = genesis_propose(&mut book, &cfg, &ctx).unwrap();
        let proposal = actions
            .iter()
            .find_map(|a| match a {
                Action::BroadcastVertexProposal(p) => Some(p.clone()),
                _ => None,
            })
            .expect("genesis proposal");
        assert_eq!(proposal.vertex.round, Round(0));
        assert_eq!(proposal.vertex.author, fix.ring.id(0));
        assert!(proposal.vertex.parents.is_empty());
        assert!(verify::verify_proposal(&fix.ring.set, &proposal));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::ScheduleTimer { .. })),
            "round fallback timer armed"
        );
        // self-partial seeded, not broadcast
        assert_eq!(book.collecting[&proposal.vertex.hash].len(), 1);
        assert!(!actions
            .iter()
            .any(|a| matches!(a, Action::BroadcastVertexPartial(_))));
        // idempotent
        assert!(genesis_propose(&mut book, &cfg, &ctx).unwrap().is_empty());
    }

    #[test]
    fn quorum_certs_advance_round_with_author_ordered_parents() {
        let fix = Fix::new();
        ctx!(fix, 0, valset, signer, parts);
        let mut book = VertexBook::new(fix.ring.id(0));
        let cfg = Config::default_table_17_1();
        let _ = genesis_propose(&mut book, &cfg, &ctx).unwrap();

        // two peer certs: not enough (own cert not yet formed → 2 < 3)
        let mut actions = Actions::new();
        on_certified_vertex(&mut book, &cfg, &cert_for(&fix.ring, 1, 0), &ctx, &mut actions)
            .unwrap();
        on_certified_vertex(&mut book, &cfg, &cert_for(&fix.ring, 2, 0), &ctx, &mut actions)
            .unwrap();
        assert_eq!(book.current_round(), 0);
        assert!(actions.is_empty());

        // third cert reaches 2f+1 → propose round 1
        on_certified_vertex(&mut book, &cfg, &cert_for(&fix.ring, 3, 0), &ctx, &mut actions)
            .unwrap();
        assert_eq!(book.current_round(), 1);
        let proposal = actions
            .iter()
            .find_map(|a| match a {
                Action::BroadcastVertexProposal(p) => Some(p.clone()),
                _ => None,
            })
            .expect("round-1 proposal");
        assert_eq!(proposal.vertex.round, Round(1));
        assert_eq!(proposal.vertex.parents.len(), 3);
        // parents = cert hashes ordered by author id (BTreeMap order)
        let expected: Vec<Hash32> = book.certified_by_round[&Round(0)]
            .values()
            .copied()
            .take(3)
            .collect();
        assert_eq!(proposal.vertex.parents, expected);
        // duplicate cert does not double-advance
        let mut again = Actions::new();
        on_certified_vertex(&mut book, &cfg, &cert_for(&fix.ring, 3, 0), &ctx, &mut again)
            .unwrap();
        assert_eq!(book.current_round(), 1);
        assert!(again.is_empty());
    }

    #[test]
    fn timer_fire_rebroadcasts_same_proposal_and_never_jumps() {
        let fix = Fix::new();
        ctx!(fix, 0, valset, signer, parts);
        let mut book = VertexBook::new(fix.ring.id(0));
        let cfg = Config::default_table_17_1();
        let genesis = genesis_propose(&mut book, &cfg, &ctx).unwrap();
        let first = genesis
            .iter()
            .find_map(|a| match a {
                Action::BroadcastVertexProposal(p) => Some(p.clone()),
                _ => None,
            })
            .unwrap();
        let timer_id = book.round_timer.expect("armed");

        let mut actions = Actions::new();
        on_timer_fired(&mut book, &cfg, &ctx, timer_id, &mut actions).unwrap();
        assert_eq!(book.current_round(), 0, "no round jump");
        assert_eq!(book.rounds_stalled(), 1);
        let rebroadcast = actions
            .iter()
            .find_map(|a| match a {
                Action::BroadcastVertexProposal(p) => Some(p.clone()),
                _ => None,
            })
            .expect("re-broadcast");
        assert_eq!(rebroadcast.vertex.hash, first.vertex.hash, "immutable within round");
        // re-armed with linear backoff: 2 × round_duration on first retry
        let Some(Action::ScheduleTimer { delay_nanos, .. }) = actions
            .iter()
            .find(|a| matches!(a, Action::ScheduleTimer { .. }))
        else {
            panic!("timer re-armed");
        };
        assert_eq!(
            *delay_nanos,
            u128::from(cfg.timing.round_duration_ms) * 1_000_000 * 2
        );
        // a stale/foreign timer id is ignored
        let mut noop = Actions::new();
        on_timer_fired(&mut book, &cfg, &ctx, TimerId(9_999), &mut noop).unwrap();
        assert!(noop.is_empty());
    }

    #[test]
    fn old_rounds_are_pruned_after_advance() {
        let fix = Fix::new();
        ctx!(fix, 0, valset, signer, parts);
        let mut book = VertexBook::new(fix.ring.id(0));
        let cfg = Config::default_table_17_1();
        let _ = genesis_propose(&mut book, &cfg, &ctx).unwrap();
        let mut actions = Actions::new();
        for round in 0u64..3 {
            for author in 1u8..4 {
                on_certified_vertex(
                    &mut book,
                    &cfg,
                    &cert_for(&fix.ring, author, round),
                    &ctx,
                    &mut actions,
                )
                .unwrap();
            }
        }
        assert_eq!(book.current_round(), 3);
        // rounds below current-1 = 2 pruned
        assert!(!book.certified_by_round.contains_key(&Round(0)));
        assert!(!book.certified_by_round.contains_key(&Round(1)));
        assert!(book.certified_by_round.contains_key(&Round(2)));
        assert!(book.my_proposals.values().all(|p| p.vertex.round.0 + 1 >= 3));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p consensus vertex_cert --locked` → compile FAIL (`genesis_propose` etc. missing).

- [ ] **Step 3: Implement (replace the Task-8 `maybe_advance` stub)**

```rust
fn round_delay_nanos(cfg: &Config, retries: u32) -> u128 {
    u128::from(cfg.timing.round_duration_ms) * 1_000_000 * (u128::from(retries) + 1)
}

/// Build, sign, seed, and broadcast this node's proposal for `round`.
fn propose_round(
    book: &mut VertexBook,
    cfg: &Config,
    ctx: &HostContext<'_>,
    round: Round,
    parents: Vec<Hash32>,
    actions: &mut Actions,
) -> Result<()> {
    let mut vertex = Vertex {
        round,
        author: book.self_id,
        parents,
        blobs: ctx.pending_blobs.drain(),
        hash: Hash32::zero(),
    };
    dag::signing::seal_hash(&mut vertex);
    let msg = dag::signing::signing_bytes(&vertex);
    let proposal = VertexProposal {
        vertex: vertex.clone(),
        proposer_sig: ctx.signer.sign_bls(dst::VERTEX_PROPOSAL, &msg),
    };
    // Self-vote: seed our own partial directly — never broadcast it,
    // since only this node aggregates this vertex.
    let self_sig = ctx.signer.sign_bls(dst::VERTEX_CERT, &msg);
    book.my_proposals.insert(vertex.hash, proposal.clone());
    book.collecting
        .entry(vertex.hash)
        .or_default()
        .insert(book.self_id, self_sig);
    book.voted.insert((round, book.self_id));
    book.proposals_seen
        .entry((round, book.self_id))
        .or_default()
        .push(proposal.clone());
    book.current_round = round;

    if let Some(old) = book.round_timer.take() {
        actions.push(Action::CancelTimer(old));
    }
    let id = book.timers.allocate();
    book.round_timer = Some(id);
    book.timer_retries = 0;
    actions.push(Action::ScheduleTimer {
        id,
        delay_nanos: round_delay_nanos(cfg, 0),
    });
    actions.push(Action::BroadcastVertexProposal(proposal));
    // n = 1 devnet: the self-partial alone is already a quorum.
    try_finalize(book, cfg, ctx, vertex.hash, actions)?;
    Ok(())
}

/// Cert-driven advancement: while round `current_round` holds ≥2f+1
/// certs, propose `current_round + 1` with those certs as parents
/// (first 2f+1 hashes in author order). Idempotent. Prunes old state.
fn maybe_advance(
    book: &mut VertexBook,
    cfg: &Config,
    ctx: &HostContext<'_>,
    actions: &mut Actions,
) -> Result<()> {
    if !book.started {
        return Ok(()); // nothing to advance before genesis_propose
    }
    let set = active_set(ctx)?;
    let need = quorum_need(&set)?;
    loop {
        let r = book.current_round;
        let Some(certs) = book.certified_by_round.get(&r) else {
            break;
        };
        if certs.len() < need {
            break;
        }
        let parents: Vec<Hash32> = certs.values().copied().take(need).collect();
        propose_round(book, cfg, ctx, Round(r.0 + 1), parents, actions)?;
    }
    // Prune everything below current_round - 1 (memory bound §4).
    let keep_from = book.current_round.0.saturating_sub(1);
    book.certified_by_round.retain(|r, _| r.0 >= keep_from);
    book.my_proposals.retain(|_, p| p.vertex.round.0 >= keep_from);
    let live: std::collections::HashSet<Hash32> =
        book.my_proposals.keys().copied().collect();
    book.collecting.retain(|h, _| live.contains(h));
    book.emitted_certs.retain(|h| live.contains(h));
    book.proposals_seen.retain(|(r, _), _| r.0 >= keep_from);
    book.voted.retain(|(r, _)| r.0 >= keep_from);
    Ok(())
}

/// One-shot bootstrap: propose round 0 with no parents.
pub fn genesis_propose(
    book: &mut VertexBook,
    cfg: &Config,
    ctx: &HostContext<'_>,
) -> Result<Actions> {
    let mut actions = Actions::new();
    if book.started {
        return Ok(actions);
    }
    book.started = true;
    propose_round(book, cfg, ctx, Round(0), vec![], &mut actions)?;
    Ok(actions)
}

/// Observe any certified vertex (own loopback or peer) and try to advance.
pub fn on_certified_vertex(
    book: &mut VertexBook,
    cfg: &Config,
    cv: &CertifiedVertex,
    ctx: &HostContext<'_>,
    actions: &mut Actions,
) -> Result<()> {
    record_cert(book, cv);
    maybe_advance(book, cfg, ctx, actions)
}

/// Round fallback: re-broadcast the current proposal, re-arm with linear
/// backoff (capped 8×). Never jumps rounds — parents must stay certified.
pub fn on_timer_fired(
    book: &mut VertexBook,
    cfg: &Config,
    ctx: &HostContext<'_>,
    id: TimerId,
    actions: &mut Actions,
) -> Result<()> {
    if book.round_timer != Some(id) {
        return Ok(());
    }
    book.round_timer = None;
    let set = active_set(ctx)?;
    let need = quorum_need(&set)?;
    if book.certified_count_at(book.current_round) >= need {
        // Quorum already reached; advancement rides the cert events.
        return Ok(());
    }
    book.rounds_stalled += 1;
    if let Some(p) = book
        .my_proposals
        .values()
        .find(|p| p.vertex.round == book.current_round)
    {
        actions.push(Action::BroadcastVertexProposal(p.clone()));
    }
    book.timer_retries = (book.timer_retries + 1).min(8);
    let tid = book.timers.allocate();
    book.round_timer = Some(tid);
    actions.push(Action::ScheduleTimer {
        id: tid,
        delay_nanos: round_delay_nanos(cfg, book.timer_retries),
    });
    Ok(())
}
```

Note on the `quorum_certs_advance...` test: validator 0's own round-0 cert never forms there (no partials are fed), so advancement is driven purely by the three peer certs — that is intended: `certified_by_round` counts **any** author's cert at the round, matching the spec's "2f+1 certificates from round r".

- [ ] **Step 4: Run the whole module**

Run: `cargo test -p consensus vertex_cert --locked`
Expected: all proposal/partial/advance tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/consensus/src/vertex_cert/mod.rs
git commit -m "feat(consensus): cert-driven round advancement, genesis, fallback timer"
```

---

## Task 10: `StateMachine` integration + 4-validator step-level test

**Files:**
- Modify: `crates/consensus/src/state_machine.rs`
- Test: `crates/consensus/tests/vertex_cert_distributed.rs` (create)

- [ ] **Step 1: Route events (replace the Task-4 stub arms)**

In `StateMachine`: add the field + import:

```rust
use crate::{
    // ...existing...
    vertex_cert::VertexBook,
};

pub struct StateMachine {
    cfg: Config,
    emitted: EmittedSet,
    waves: WaveBook,
    macros: MacroBook,
    /// L1 distributed vertex certification (06-04 design).
    vertices: VertexBook,
}
```

In `new`: `vertices: VertexBook::new(self_id),`.

Replace the `Event::CertifiedVertexReceived` arm:

```rust
            Event::CertifiedVertexReceived(cv) => {
                let mut actions = Actions::new();
                crate::vertex_cert::on_certified_vertex(
                    &mut self.vertices,
                    &self.cfg,
                    &cv,
                    ctx,
                    &mut actions,
                )?;
                let bull = crate::bullshark::on_certified_vertex(
                    &mut self.emitted,
                    &mut self.waves,
                    &self.cfg,
                    cv,
                    ctx,
                )?;
                actions.extend(bull);
                crate::macro_fin::on_local_micro_qcs(
                    &mut self.macros,
                    &self.cfg,
                    ctx,
                    &mut actions,
                )?;
                Ok(actions)
            }
```

Extend the `Event::TimerFired` arm (after the macro_fin call):

```rust
                crate::vertex_cert::on_timer_fired(
                    &mut self.vertices,
                    &self.cfg,
                    ctx,
                    id,
                    &mut actions,
                )?;
```

Replace the Task-4 stub arms:

```rust
            Event::VertexProposalReceived(p) => crate::vertex_cert::on_vertex_proposal(
                &mut self.vertices,
                &self.cfg,
                p,
                ctx,
            ),
            Event::VertexPartialReceived(bp) => crate::vertex_cert::on_vertex_partial(
                &mut self.vertices,
                &self.cfg,
                bp,
                ctx,
            ),
```

Add the public entry points:

```rust
    /// Bootstrap the distributed L1 path: propose the round-0 vertex.
    /// Idempotent; hosts call it once before entering the event loop.
    pub fn genesis_propose(&mut self, ctx: &HostContext<'_>) -> Result<Actions> {
        crate::vertex_cert::genesis_propose(&mut self.vertices, &self.cfg, ctx)
    }

    /// Round of this node's latest own vertex proposal (sim/test probe).
    #[must_use]
    pub fn current_vertex_round(&self) -> u64 {
        self.vertices.current_round()
    }
```

- [ ] **Step 2: Write the 4-validator handshake test**

Create `crates/consensus/tests/vertex_cert_distributed.rs`:

```rust
//! Four StateMachines complete genesis → partials → certs → round 1
//! purely through `step` calls (no network, no host).

use std::collections::VecDeque;

use consensus::{
    Config, Event, HostContext, StateMachine,
    action::Action,
    ports::{
        Clock, DagView, NoPendingBlobs, Persistence, RandomnessBeacon, SignerPort,
        ValidatorSetPort,
    },
};
use crypto::bls::keys::SecretKey;
use types::{
    crypto_types::{BlsSig, Hash32, VrfProof, VrfPubkey},
    primitives::{Epoch, Height, Round, StakeWeight, ValidatorId},
    validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
};

struct Ring {
    sks: Vec<SecretKey>,
    set: ValidatorSet,
}

impl Ring {
    fn new(n: u8) -> Self {
        let sks: Vec<SecretKey> = (0..n)
            .map(|i| SecretKey::from_ikm(&[i + 1; 32]).unwrap())
            .collect();
        let entries = (0..n)
            .map(|i| ValidatorEntry {
                id: ValidatorId([i; 32]),
                bls_pubkey: sks[i as usize].public().to_bytes(),
                vrf_pubkey: VrfPubkey::zero(),
                stake: StakeWeight(1),
                identity: ValidatorIdentity {
                    asn: None,
                    cloud: None,
                    region: None,
                },
            })
            .collect();
        Self {
            sks,
            set: ValidatorSet {
                epoch: Epoch(0),
                entries,
                total_stake: StakeWeight(u64::from(n)),
            },
        }
    }
}

struct RingSigner<'a> {
    ring: &'a Ring,
    idx: usize,
}

impl SignerPort for RingSigner<'_> {
    fn sign_bls(&self, dst: &[u8], msg: &[u8]) -> BlsSig {
        crypto::bls::sign::sign(&self.ring.sks[self.idx], dst, msg)
    }
    fn vrf_prove(&self, _alpha: &[u8]) -> consensus::Result<(VrfProof, Hash32)> {
        Ok((VrfProof::zero(), Hash32::zero()))
    }
}

struct FixedValset(ValidatorSet);
impl ValidatorSetPort for FixedValset {
    fn set_for(&self, epoch: Epoch) -> consensus::Result<Option<ValidatorSet>> {
        Ok((self.0.epoch == epoch).then(|| self.0.clone()))
    }
    fn index_of(&self, _e: Epoch, v: &ValidatorId) -> consensus::Result<Option<u32>> {
        Ok(self
            .0
            .entries
            .iter()
            .position(|x| &x.id == v)
            .map(|i| u32::try_from(i).unwrap()))
    }
}

struct EmptyDag;
impl DagView for EmptyDag {
    fn vertex(&self, _h: &Hash32) -> consensus::Result<Option<types::dag::CertifiedVertex>> {
        Ok(None)
    }
    fn vertices_at_round(
        &self,
        _r: Round,
    ) -> consensus::Result<Vec<types::dag::CertifiedVertex>> {
        Ok(vec![])
    }
}

struct ZeroClock;
impl Clock for ZeroClock {
    fn now_nanos(&self) -> u128 {
        0
    }
}

struct ZeroBeacon;
impl RandomnessBeacon for ZeroBeacon {
    fn current(&self) -> consensus::Result<Hash32> {
        Ok(Hash32::zero())
    }
}

struct NoopPersistence;
impl Persistence for NoopPersistence {
    fn store_micro_qc(&self, _q: &types::micro::MicroQc) -> consensus::Result<()> {
        Ok(())
    }
    fn micro_qc_for(&self, _h: &Hash32) -> consensus::Result<Option<types::micro::MicroQc>> {
        Ok(None)
    }
    fn store_macro_checkpoint(
        &self,
        _c: &types::macros::MacroCheckpoint,
    ) -> consensus::Result<()> {
        Ok(())
    }
    fn store_macro_qc(&self, _q: &types::macros::MacroQc) -> consensus::Result<()> {
        Ok(())
    }
    fn append_slash_evidence(
        &self,
        _e: &types::slashing::SlashEvidence,
    ) -> consensus::Result<()> {
        Ok(())
    }
    fn macro_checkpoint_at(
        &self,
        _h: Height,
    ) -> consensus::Result<Option<types::macros::MacroCheckpoint>> {
        Ok(None)
    }
    fn macro_qc_for(&self, _h: &Hash32) -> consensus::Result<Option<types::macros::MacroQc>> {
        Ok(None)
    }
}

/// Route actions: every Broadcast* becomes the matching Event for all
/// OTHER machines; BroadcastCertifiedVertex also loops back to the
/// sender (orchestrator-loopback semantics).
fn route(sender: usize, actions: consensus::state_machine::Actions, queue: &mut VecDeque<(usize, Event)>, n: usize) {
    for action in actions {
        match action {
            Action::BroadcastVertexProposal(p) => {
                for i in (0..n).filter(|&i| i != sender) {
                    queue.push_back((i, Event::VertexProposalReceived(p.clone())));
                }
            }
            Action::BroadcastVertexPartial(bp) => {
                for i in (0..n).filter(|&i| i != sender) {
                    queue.push_back((i, Event::VertexPartialReceived(bp.clone())));
                }
            }
            Action::BroadcastCertifiedVertex(cv) => {
                for i in 0..n {
                    queue.push_back((i, Event::CertifiedVertexReceived(cv.clone())));
                }
            }
            _ => {}
        }
    }
}

#[test]
fn four_validators_certify_genesis_and_advance_to_round_one() {
    let n = 4usize;
    let ring = Ring::new(4);
    let valset = FixedValset(ring.set.clone());
    let (dag, clock, beacon, persist, no_pending) =
        (EmptyDag, ZeroClock, ZeroBeacon, NoopPersistence, NoPendingBlobs);

    let mut machines: Vec<StateMachine> = (0..n)
        .map(|i| StateMachine::new(Config::default_table_17_1(), ValidatorId([u8::try_from(i).unwrap(); 32])))
        .collect();

    let mut queue: VecDeque<(usize, Event)> = VecDeque::new();
    for (i, m) in machines.iter_mut().enumerate() {
        let signer = RingSigner { ring: &ring, idx: i };
        let ctx = HostContext {
            dag: &dag,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let actions = m.genesis_propose(&ctx).unwrap();
        route(i, actions, &mut queue, n);
    }

    let mut steps = 0usize;
    while let Some((i, event)) = queue.pop_front() {
        steps += 1;
        assert!(steps < 10_000, "message storm — protocol not converging");
        let signer = RingSigner { ring: &ring, idx: i };
        let ctx = HostContext {
            dag: &dag,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let actions = machines[i].step(event, &ctx).unwrap();
        route(i, actions, &mut queue, n);
    }

    for (i, m) in machines.iter().enumerate() {
        assert!(
            m.current_vertex_round() >= 1,
            "validator {i} stuck at round {}",
            m.current_vertex_round()
        );
    }
}
```

- [ ] **Step 3: Run**

Run: `cargo test -p consensus --locked`
Expected: the handshake test PASSES (each machine certifies its genesis vertex via 3 peer partials, sees ≥3 certs at round 0, proposes round 1 — the queue drains because round-1 proposals from peers are held, not re-queued, once parents are missing... they are *not* re-queued: held proposals produce zero actions, so the loop terminates) and **all** pre-existing consensus tests stay green.

- [ ] **Step 4: Commit**

```bash
git add crates/consensus/src/state_machine.rs crates/consensus/tests/vertex_cert_distributed.rs
git commit -m "feat(consensus): route vertex_cert through StateMachine; 4-validator handshake test"
```

---

## Task 11: Node host — `vertex_protocol` flag, orchestrator genesis + loopback, integration test

**Files:**
- Modify: `apps/node/src/config_layers.rs`
- Modify: `apps/node/src/orchestrator.rs`
- Modify: `apps/node/src/runtime.rs`
- Modify: `config/profiles/devnet.toml`
- Test: `apps/node/tests/l1_distributed_smoke.rs` (create)

- [ ] **Step 1: Config flag (`config_layers.rs`)**

Add to `NodeSection` (after `l1_real_vertex_certs`):

```rust
    /// Which L1 vertex production path runs (06-04 design):
    /// `"distributed"` = propose → partials → 2f+1 CV (production);
    /// `"devnet_factory"` = legacy L1Driver fabrication (default).
    #[serde(default)]
    pub vertex_protocol: VertexProtocol,
```

and the enum (top level of the file):

```rust
/// L1 vertex production path selector (06-04 design §5).
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VertexProtocol {
    /// Distributed propose/partial/aggregate protocol.
    Distributed,
    /// Legacy host-side devnet factory (`L1Driver`).
    #[default]
    DevnetFactory,
}
```

`config/profiles/devnet.toml` — next to `l1_driver_enabled`:

```toml
# L1 vertex production: "devnet_factory" (legacy driver) | "distributed" (06-04)
vertex_protocol = "devnet_factory"
```

- [ ] **Step 2: Orchestrator — genesis kick + cert loopback (`orchestrator.rs`)**

Add a field + constructor parameter (callers updated in Step 3):

```rust
    /// Distributed L1 path active: genesis-propose at startup and loop
    /// own certified vertices back as local events.
    vertex_protocol_distributed: bool,
```

Extract action dispatch from `run` into a method and add the loopback:

```rust
    fn dispatch_actions(&mut self, actions: consensus::state_machine::Actions) {
        for action in actions {
            self.metrics.actions_dispatched.inc();
            if let Action::BroadcastCertifiedVertex(cv) = &action {
                // gossipsub never delivers our own publish: loop the cert
                // back so LiveDag + Bullshark + vertex_cert all see it.
                if self
                    .bridge
                    .events_tx
                    .try_send(Event::CertifiedVertexReceived(cv.clone()))
                    .is_err()
                {
                    warn!(
                        target: "node::orchestrator",
                        "events channel full; dropping own-cert loopback"
                    );
                }
            }
            if net::gossip_wire::is_broadcast(&action) {
                if let Err(e) = self.net_actions_tx.try_send(action.clone()) {
                    self.metrics.actions_dropped.inc();
                    warn!(target: "node::orchestrator", error = %e, "net actions channel full; dropping broadcast");
                }
            }
            if let Err(e) = self.action_applier.apply(&action) {
                warn!(target: "node::orchestrator", error = %e, "local action apply failed");
            }
        }
    }
```

In `run`, before the loop:

```rust
        if self.vertex_protocol_distributed {
            let ctx = crate::host_context::build_host_context(
                &self.host_bundle,
                &self.persistence,
            );
            match self.sm.genesis_propose(&ctx) {
                Ok(actions) => self.dispatch_actions(actions),
                Err(e) => warn!(target: "node::orchestrator", error = %e, "genesis propose failed"),
            }
        }
```

and replace the inline `for action in actions { ... }` body with `self.dispatch_actions(actions);`.

- [ ] **Step 3: Runtime gating (`runtime.rs`)**

Replace the `if cfg.node.l1_driver_enabled { ... }` block with:

```rust
    match cfg.node.vertex_protocol {
        crate::config_layers::VertexProtocol::Distributed => {
            anyhow::ensure!(
                cfg.node.l1_real_vertex_certs,
                "vertex_protocol=\"distributed\" requires l1_real_vertex_certs=true"
            );
            anyhow::ensure!(
                gossip_publish_tx.is_some(),
                "vertex_protocol=\"distributed\" requires a live gossip swarm"
            );
            if cfg.node.l1_driver_enabled {
                warn!(
                    target: "node",
                    "l1_driver_enabled ignored: vertex_protocol=\"distributed\" owns L1 produce"
                );
            }
            info!(target: "node", "L1 distributed vertex certification active");
        }
        crate::config_layers::VertexProtocol::DevnetFactory => {
            if cfg.node.l1_driver_enabled {
                let publish_tx = gossip_publish_tx.with_context(|| {
                    "l1_driver_enabled requires a live gossip swarm (not skeleton network mode)"
                })?;
                let round_ms = cfg.consensus.timing.round_duration_ms;
                let driver = L1Driver::new(
                    valset.clone(),
                    cfg.consensus.clone(),
                    Arc::clone(&live_dag),
                    Arc::clone(&host_bundle.beacon),
                    events_tx.clone(),
                    publish_tx,
                    std::time::Duration::from_millis(round_ms),
                    cfg.node.l1_real_vertex_certs,
                    if cfg.node.l1_blob_custody_enabled {
                        blob_custody_handle.clone()
                    } else {
                        None
                    },
                    metrics.clone(),
                );
                tokio::spawn(async move {
                    driver.run().await;
                });
                info!(target: "node", round_duration_ms = round_ms, "L1 driver started");
            }
        }
    }
```

Pass the flag into the orchestrator:

```rust
    let orch = Orchestrator::new(
        sm,
        bridge,
        events_rx,
        persistence,
        metrics,
        net_actions_tx,
        host_bundle,
        action_applier,
        valset,
        cfg.node.l1_real_vertex_certs,
        cfg.node.vertex_protocol == crate::config_layers::VertexProtocol::Distributed,
    );
```

Update every other `Orchestrator::new` call site (`rg "Orchestrator::new"`) — append `false` (tests exercising the legacy path: `l1_driver_smoke.rs` and any others found).

- [ ] **Step 4: Integration test**

Create `apps/node/tests/l1_distributed_smoke.rs`:

```rust
//! Distributed L1 smoke: orchestrator genesis-proposes, two injected peer
//! partials complete the quorum, the cert broadcasts and self-ingests.

use std::sync::Arc;
use std::time::{Duration, Instant};

use consensus::{StateMachine, action::Action, event::Event};
use crypto::hash::dst;
use net::Bridge;
use node::{
    action_applier::ActionApplier,
    blob::{BlobCustody, BlobCustodyConfig, RocksBlobStore},
    devnet_keys::{devnet_bls_ikm, devnet_valset_four, validator_id_from_label},
    host_context::StubHostBundle,
    live_dag::LiveDag,
    observability::metrics::Metrics,
    orchestrator::Orchestrator,
    timer::TimerRegistry,
};
use storage::{Database, RocksPersistence, config::StorageConfig};
use tokio::sync::mpsc;
use types::{
    dag::{VertexPartial, VertexProposal},
    primitives::Round,
};

struct Node0 {
    net_actions_rx: mpsc::Receiver<Action>,
    events_tx: mpsc::Sender<Event>,
    live_dag: Arc<LiveDag>,
    _dir: tempfile::TempDir,
}

async fn spawn_node0(custody_blob: Option<Vec<u8>>) -> Node0 {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(
        Database::open(&StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 16,
        })
        .unwrap(),
    );
    let live_dag = Arc::new(LiveDag::new(Arc::clone(&db)));
    let persistence = RocksPersistence::new(Arc::clone(&db));
    let valset = devnet_valset_four();
    let cfg = consensus::Config::default_table_17_1();
    let self_id = validator_id_from_label("node0");

    let (events_tx, events_rx) = mpsc::channel(1024);
    let (bridge, _bridge_handle) = Bridge::with_channels(events_tx.clone(), 1024);
    let (net_actions_tx, net_actions_rx) = mpsc::channel(1024);
    let metrics = Arc::new(Metrics::new().unwrap());

    // Optional blob custody, pre-loaded BEFORE the orchestrator genesis.
    let custody = if let Some(payload) = custody_blob {
        let store = Arc::new(RocksBlobStore::new(Arc::clone(&db)))
            as Arc<dyn dag::blob::store::BlobStore>;
        let (_chunks_tx, chunks_rx) = mpsc::channel(64);
        let (publish_tx, mut publish_rx) = mpsc::channel(256);
        tokio::spawn(async move { while publish_rx.recv().await.is_some() {} });
        let handle = BlobCustody::spawn(
            store,
            chunks_rx,
            publish_tx,
            BlobCustodyConfig {
                chunk_size: 1024,
                erasure: None,
            },
            metrics.clone(),
        );
        handle.publish_payload(payload).await.unwrap();
        Some(handle)
    } else {
        None
    };

    let timer_registry = Arc::new(TimerRegistry::default());
    let (timer_schedule_tx, mut timer_schedule_rx) = mpsc::channel(256);
    let events_tx_timer = events_tx.clone();
    let registry_for_loop = timer_registry.clone();
    tokio::spawn(async move {
        while let Some((id, delay)) = timer_schedule_rx.recv().await {
            node::timer::schedule_event(&registry_for_loop, events_tx_timer.clone(), id, delay);
        }
    });

    let sm = StateMachine::new(cfg, self_id);
    let host_bundle =
        StubHostBundle::new("node0", valset.clone(), Arc::clone(&live_dag), None, custody)
            .unwrap();
    let beacon = Arc::clone(&host_bundle.beacon);
    let action_applier = ActionApplier::new(
        persistence.clone(),
        timer_schedule_tx,
        timer_registry,
        beacon,
        metrics.clone(),
    );
    let orch = Orchestrator::new(
        sm,
        bridge,
        events_rx,
        persistence,
        metrics,
        net_actions_tx,
        host_bundle,
        action_applier,
        valset,
        true, // l1_real_vertex_certs
        true, // vertex_protocol distributed
    );
    tokio::spawn(orch.run());
    Node0 {
        net_actions_rx,
        events_tx,
        live_dag,
        _dir: dir,
    }
}

async fn next_action<F: Fn(&Action) -> bool>(
    rx: &mut mpsc::Receiver<Action>,
    want: F,
    what: &str,
) -> Action {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        match tokio::time::timeout(Duration::from_millis(250), rx.recv()).await {
            Ok(Some(a)) if want(&a) => return a,
            Ok(Some(_)) => {}
            Ok(None) => panic!("net actions channel closed waiting for {what}"),
            Err(_) => {}
        }
    }
}

fn peer_partial(label: &str, proposal: &VertexProposal) -> VertexPartial {
    let sk = crypto::bls::SecretKey::from_ikm(&devnet_bls_ikm(label)).unwrap();
    let msg = dag::signing::signing_bytes(&proposal.vertex);
    VertexPartial {
        vertex_hash: proposal.vertex.hash,
        round: proposal.vertex.round,
        author: proposal.vertex.author,
        voter: validator_id_from_label(label),
        sig: crypto::bls::sign::sign(&sk, dst::VERTEX_CERT, &msg),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn genesis_proposal_plus_two_peer_partials_yield_verified_cert() {
    let mut node = spawn_node0(None).await;
    let valset = devnet_valset_four();

    let action = next_action(
        &mut node.net_actions_rx,
        |a| matches!(a, Action::BroadcastVertexProposal(_)),
        "genesis proposal",
    )
    .await;
    let Action::BroadcastVertexProposal(proposal) = action else {
        unreachable!()
    };
    assert_eq!(proposal.vertex.round, Round(0));
    assert_eq!(proposal.vertex.author, validator_id_from_label("node0"));
    assert!(proposal.vertex.parents.is_empty());

    // A forged partial must NOT complete the quorum.
    let mut forged = peer_partial("node1", &proposal);
    forged.sig = types::crypto_types::BlsSig([0xEE; 96]);
    node.events_tx
        .send(Event::VertexPartialReceived(forged))
        .await
        .unwrap();
    // One honest partial: self + node1 = 2 < 3 → still no cert.
    node.events_tx
        .send(Event::VertexPartialReceived(peer_partial("node1", &proposal)))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        node.live_dag.vertices_at_round(Round(0)).unwrap().is_empty(),
        "no cert below quorum / from forged partials"
    );

    // Second honest partial reaches 2f+1 = 3.
    node.events_tx
        .send(Event::VertexPartialReceived(peer_partial("node2", &proposal)))
        .await
        .unwrap();
    let action = next_action(
        &mut node.net_actions_rx,
        |a| matches!(a, Action::BroadcastCertifiedVertex(_)),
        "certified vertex",
    )
    .await;
    let Action::BroadcastCertifiedVertex(cv) = action else {
        unreachable!()
    };
    dag::cert::verify_certified_vertex(&cv, &valset).expect("broadcast cert verifies");
    assert_eq!(cv.vertex.hash, proposal.vertex.hash);

    // Loopback self-ingest: the cert lands in LiveDag.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if !node.live_dag.vertices_at_round(Round(0)).unwrap().is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "own cert never self-ingested");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pending_blob_rides_in_genesis_proposal() {
    let mut node = spawn_node0(Some(vec![0xA5; 1500])).await;
    let action = next_action(
        &mut node.net_actions_rx,
        |a| matches!(a, Action::BroadcastVertexProposal(_)),
        "genesis proposal with blob",
    )
    .await;
    let Action::BroadcastVertexProposal(proposal) = action else {
        unreachable!()
    };
    assert_eq!(proposal.vertex.blobs.len(), 1, "drained pending BlobRef");
}
```

- [ ] **Step 5: Run node tests**

Run: `cargo test -p node --locked`
Expected: both new tests PASS; `l1_driver_smoke` (legacy path, flag `false`) still PASSES.

- [ ] **Step 6: Commit**

```bash
git add apps/node/src apps/node/tests config/profiles/devnet.toml
git commit -m "feat(node): vertex_protocol flag, orchestrator genesis kick and cert loopback"
```

---

## Task 12: Sim — distributed vertex production mode + scenario

**Files:**
- Modify: `apps/sim/src/world.rs`
- Modify: `apps/sim/src/args.rs`, `apps/sim/src/scenarios/mod.rs`
- Create: `apps/sim/src/scenarios/vertex_cert_distributed.rs`

- [ ] **Step 1: World flag + distributed tick**

`apps/sim/src/world.rs` — add a field to `World`:

```rust
    /// When true, vertices come from the distributed vertex_cert protocol
    /// (genesis_propose + gossip) instead of the exogenous factory.
    pub distributed_vertices: bool,
```

Initialize `distributed_vertices: false` in `World::new`. Add:

```rust
    /// Switch to distributed vertex production (06-04). Call before `run`.
    pub fn enable_distributed_vertices(&mut self) {
        self.distributed_vertices = true;
    }

    /// Genesis-propose on every machine (distributed mode bootstrap).
    fn genesis_propose_all(&mut self, now: u64) {
        for idx in 0..u32::try_from(self.machines.len()).expect("validator count") {
            let i = idx as usize;
            let signer = self.key_ring.signer(i);
            let no_pending = consensus::ports::NoPendingBlobs;
            let ctx = HostContext {
                dag: self.dag.as_ref(),
                clock: self.clock.as_ref(),
                valset: self.valset.as_ref(),
                beacon: self.beacon.as_ref(),
                persistence: self.persistence[i].as_ref(),
                signer: &signer,
                pending_blobs: &no_pending,
            };
            let actions = self.machines[i]
                .genesis_propose(&ctx)
                .unwrap_or_else(|e| panic!("validator {idx} genesis failed: {e}"));
            self.apply_actions(idx, actions, now);
        }
    }
```

Modify `tick_round` to branch:

```rust
    pub fn tick_round(&mut self) {
        let now = u64::try_from(self.clock.as_ref().now_nanos()).unwrap_or(u64::MAX);
        self.drain_net_and_apply(now);
        if self.distributed_vertices {
            if self.virtual_round == 0 {
                self.genesis_propose_all(now);
            }
        } else {
            self.produce_vertex_tick(now);
        }
        self.drain_timers_and_apply(now);
        let round_nanos = self.config.timing.round_duration_ms * 1_000_000;
        self.clock.advance(round_nanos);
        self.virtual_round += 1;
    }
```

Add a probe used by the scenario:

```rust
    /// Minimum own-proposal round across all machines (distributed mode).
    #[must_use]
    pub fn min_vertex_round(&self) -> u64 {
        self.machines
            .iter()
            .map(consensus::StateMachine::current_vertex_round)
            .min()
            .unwrap_or(0)
    }

    /// Total slash evidence records across all validators' persistence.
    #[must_use]
    pub fn slash_evidence_total(&self) -> usize {
        self.persistence
            .iter()
            .map(|p| p.slash_evidence_count())
            .sum()
    }
```

(`VirtualPersistence::slash_evidence_count()` — if this accessor does not already exist, mirror the existing pattern used by `scenarios/equivocation_inject.rs`'s `slash_evidence_count()`; reuse that exact existing accessor instead of adding a new one if present.)

- [ ] **Step 2: Scenario plumbing**

`apps/sim/src/args.rs` — append to the `Scenario` value enum:

```rust
    /// Distributed vertex certification happy path (06-04).
    VertexCertDistributed,
```

`apps/sim/src/scenarios/mod.rs` — `pub mod vertex_cert_distributed;` + dispatch arm:

```rust
        Scenario::VertexCertDistributed => {
            vertex_cert_distributed::run(args.validators, args.rounds, seed)
        }
```

- [ ] **Step 3: Scenario + embedded adversarial tests**

Create `apps/sim/src/scenarios/vertex_cert_distributed.rs`:

```rust
//! Distributed vertex certification scenarios (06-04 design §6).

use crate::{scenarios::Report, world::World};

/// Happy path: n validators run the distributed protocol; every machine's
/// own-proposal round advances and the shared DAG accumulates certs.
#[must_use]
pub fn run(validators: u32, rounds: u32, seed: [u8; 32]) -> Report {
    let mut world = World::new(
        validators,
        seed,
        consensus::Config::default_table_17_1(),
    );
    world.enable_distributed_vertices();
    world.run(rounds);

    let min_round = world.min_vertex_round();
    let advanced = min_round >= 2;
    Report {
        scenario: "vertex-cert-distributed".into(),
        validators,
        rounds,
        safety_ok: advanced,
        liveness_ok: advanced,
        lock_macro_ok: true,
        notes: vec![format!("min own-proposal round = {min_round}")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::primitives::Round;

    #[test]
    fn happy_path_four_validators_advance() {
        let report = run(4, 32, [7; 32]);
        assert!(report.liveness_ok, "{:?}", report.notes);
    }

    #[test]
    fn all_four_authors_certify_round_zero() {
        let mut world = World::new(4, [8; 32], consensus::Config::default_table_17_1());
        world.enable_distributed_vertices();
        world.run(16);
        let certs = world.dag.vertices_at_round(Round(0)).unwrap();
        let mut authors: Vec<_> = certs.iter().map(|cv| cv.vertex.author).collect();
        authors.sort();
        authors.dedup();
        assert_eq!(authors.len(), 4, "every validator's genesis vertex certified");
    }

    #[test]
    fn partition_halts_and_heals() {
        let mut world = World::new(4, [9; 32], consensus::Config::default_table_17_1());
        world.enable_distributed_vertices();
        // split 2/2: neither side can reach 2f+1 = 3
        world.net.set_partition([0u32, 1], [2u32, 3]);
        world.run(16);
        let stalled_at = world.min_vertex_round();
        assert!(stalled_at <= 1, "split below quorum must not advance");
        world.net.heal_partition();
        world.run(32);
        assert!(
            world.min_vertex_round() > stalled_at,
            "healed network resumes advancement"
        );
    }

    #[test]
    fn double_propose_yields_vertex_equivocation_evidence() {
        use consensus::event::Event;
        use crate::virtual_net::InFlight;
        use crypto::hash::dst;
        use types::dag::{Vertex, VertexProposal};
        use types::crypto_types::Hash32;

        let mut world = World::new(4, [10; 32], consensus::Config::default_table_17_1());
        world.enable_distributed_vertices();
        world.run(2); // genesis proposals delivered

        // Forge a SECOND, conflicting round-0 proposal from validator 0
        // using its real key, and deliver it to validator 1.
        let mut vertex = Vertex {
            round: types::primitives::Round(0),
            author: crate::vertex_factory::validator_id_for_index(0),
            parents: vec![],
            blobs: vec![Default::default(); 0],
            hash: Hash32::zero(),
        };
        // differ in content: one fake parent is invalid for round 0, so
        // differ via blobs instead — empty vs a marker parent is rejected;
        // simplest conflicting content: tweak nothing structural but the
        // hash must differ, so attach one blob ref.
        vertex.blobs = vec![types::dag::BlobRef {
            blob_id: types::primitives::BlobId([0xBB; 32]),
            commitment: Hash32([0xCC; 32]),
            size_bytes: 1,
        }];
        dag::signing::seal_hash(&mut vertex);
        let msg = dag::signing::signing_bytes(&vertex);
        let sig = crypto::bls::sign::sign(
            &world.key_ring_bls_secret(0),
            dst::VERTEX_PROPOSAL,
            &msg,
        );
        world.net.enqueue(InFlight {
            recipient: 1,
            event: Event::VertexProposalReceived(VertexProposal {
                vertex,
                proposer_sig: sig,
            }),
            deliver_at: 0,
        });
        world.run(8);
        assert!(
            world.slash_evidence_total() > 0,
            "conflicting proposal must produce VertexEquivocation evidence"
        );
    }
}
```

Expose the key accessor the equivocation test needs on `World` (world.rs):

```rust
    /// Clone validator `i`'s BLS secret (adversary tests only).
    #[must_use]
    pub fn key_ring_bls_secret(&self, i: usize) -> crypto::bls::SecretKey {
        self.key_ring.bls_secret(i)
    }
```

- [ ] **Step 4: Run sim tests + scenario binary**

Run: `cargo test -p sim --locked`
Expected: 4 new tests PASS; all existing scenarios still PASS (factory path untouched — `distributed_vertices` defaults to false).
Run: `cargo run -p sim -- --scenario vertex-cert-distributed --validators 4 --rounds 32 --seed 7`
Expected: JSON report with `"liveness_ok": true`.

- [ ] **Step 5: Commit**

```bash
git add apps/sim/src
git commit -m "feat(sim): distributed vertex production mode and scenarios"
```

---

## Task 13: Docs + verification sweep

**Files:**
- Modify: `docs/superpowers/specs/2026-06-04-distributed-vertex-certificate-design.md`
- Modify: `docs/superpowers/plans/2026-05-23-06b-l1-vertex-driver.md`

- [ ] **Step 1: Spec status**

In the 06-04 spec header, change:

```markdown
**Status:** Approved (design) — implementation plan: [`2026-06-11-distributed-vertex-certificate.md`](../plans/2026-06-11-distributed-vertex-certificate.md)
```

- [ ] **Step 2: Deprecation note on the 06b driver plan**

At the top of `2026-05-23-06b-l1-vertex-driver.md` (below the title):

```markdown
> **DEPRECATION (2026-06-11):** The centralized `L1Driver` produce path this plan built is
> now the legacy `vertex_protocol = "devnet_factory"` mode. The production path is the
> distributed protocol from
> [`2026-06-04-distributed-vertex-certificate-design.md`](../specs/2026-06-04-distributed-vertex-certificate-design.md),
> implemented by [`2026-06-11-distributed-vertex-certificate.md`](2026-06-11-distributed-vertex-certificate.md).
```

- [ ] **Step 3: Final verification sweep**

```bash
cargo test --workspace --locked
rg "build_quorum_cert\b" apps/node/src        # expect: only l1/vertex_builder.rs (devnet_factory path)
rg "devnet_bls_ikm" apps/node/src             # expect: devnet_keys.rs + signer.rs only — no vertex_cert use
cargo run -p sim -- --scenario vertex-cert-distributed --validators 4 --rounds 32 --seed 7
```

All green; the sim report shows liveness.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers
git commit -m "docs: link 06-04 design to its implementation plan; deprecate central driver plan"
```

---

## Verification checklist (end state)

- [ ] `cargo test --workspace --locked` green with `vertex_protocol = "devnet_factory"` (default) — proves zero regression
- [ ] `cargo test -p consensus vertex_cert --locked` — unit handlers green
- [ ] `crates/consensus/tests/vertex_cert_distributed.rs` — 4 SMs reach round ≥ 1 with verified certs
- [ ] `apps/node/tests/l1_distributed_smoke.rs` — genesis → injected partials → `BroadcastCertifiedVertex` → LiveDag self-ingest; forged partial never certifies; pending blob rides in the proposal
- [ ] sim `vertex-cert-distributed`: happy path advances; 2/2 partition halts (no round jump) and resumes after heal; double-propose emits `VertexEquivocation`
- [ ] No node-runtime path signs with another validator's key (`rg devnet_bls_ikm apps/node/src` clean outside devnet_keys/signer)
- [ ] Orchestrator still rejects tampered CVs (`vertex_cert_rejected` metric path untouched)

## Task dependency graph

```text
Task 1 (types+dst) ──→ Task 2 (dag cert) ──→ Task 3 (slashing)
                                   │
Task 4 (enums + wire plumbing) ←───┘
Task 5 (PendingBlobSource port)
Task 4 + 5 ──→ Task 6 (book+verify) → Task 7 (proposal) → Task 8 (partial) → Task 9 (advance)
                                                            → Task 10 (SM routing + handshake test)
Task 10 ──→ Task 11 (node host) ──→ Task 13 (docs)
       └──→ Task 12 (sim)       ──↗
```
