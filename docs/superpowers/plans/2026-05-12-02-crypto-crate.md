# `crates/crypto` Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up `crates/crypto/` exposing a stable trait-fronted API over BLS12-381 (`blst`), ECVRF (Edwards25519, RFC 9381), Blake3/SHA-256 hashing, HKDF, and a DKG fingerprint stub. Every primitive has working `unimplemented!()`-free baseline semantics (signing/verification/hashing/expand) so downstream crates can call them; advanced flows (PoP rogue-key resistance edge cases, stake-weighted sortition fast path, real DKG) are explicitly stubbed with TODO references but **do not block compilation or tests**.

**Architecture:** Each primitive lives in its own submodule (`hash`, `bls`, `vrf`, `kdf`, `dkg`). Public API is a thin façade over `blst` + `curve25519-dalek` + `blake3`. Identifiers (`BlsPubkey`, `BlsSig`, `Hash32`, `VrfProof`, `Pop`) are re-exported from `crates/types` so signatures cross crate boundaries cleanly. Domain separation tags (DST) are centralised in `hash::dst`. `unsafe_code` is explicitly allowed in this crate only because `blst` requires it under FFI.

**Tech Stack:** `blst` 0.3 (BLS12-381 from Supranational), `curve25519-dalek` 4 (Edwards25519 group), `blake3` 1, `sha2` 0.10, `rand_chacha` 0.3 (deterministic RNG for tests).

**Prerequisites:** Plans 00 + 01.

---

## File Structure

Per spec §7.2.

```
crates/crypto/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── error.rs
│   ├── hash.rs                # Blake3 + SHA-256 + DST constants
│   ├── bls/
│   │   ├── mod.rs
│   │   ├── keys.rs            # SecretKey, PublicKey, Pop
│   │   ├── sign.rs            # sign / verify (single-key)
│   │   ├── aggregate.rs       # aggregate + verify_aggregate
│   │   └── bitmap.rs          # validator/subnet bitmap helpers
│   ├── vrf/
│   │   ├── mod.rs
│   │   ├── ecvrf.rs           # ECVRF prove/verify (RFC 9381)
│   │   └── sortition.rs       # stake-weighted sortition over VRF output
│   ├── kdf.rs                 # HKDF-Blake3
│   └── dkg/
│       ├── mod.rs
│       └── fingerprint.rs     # DkgCommitment fingerprint helper
├── benches/
│   ├── bls_verify.rs
│   ├── bls_aggregate.rs
│   └── vrf_verify.rs
└── tests/
    ├── bls_pop.rs
    └── vrf_determinism.rs
```

---

## Task 1: Crate skeleton + workspace registration

**Files:**
- Create: `crates/crypto/Cargo.toml`
- Create: `crates/crypto/src/lib.rs`
- Modify: `Cargo.toml` (workspace root — extend `members`)

- [ ] **Step 1: Write `crates/crypto/Cargo.toml`**

```toml
[package]
name         = "crypto"
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
borsh       = { workspace = true }
thiserror   = { workspace = true }
hex         = { workspace = true }
blake3      = { workspace = true }
sha2        = { workspace = true }
blst        = { workspace = true }
curve25519-dalek = { workspace = true, features = ["digest", "rand_core"] }
rand        = { workspace = true }
rand_chacha = { workspace = true }

[dev-dependencies]
proptest    = { workspace = true }

[[bench]]
name    = "bls_verify"
harness = false

[[bench]]
name    = "bls_aggregate"
harness = false

[[bench]]
name    = "vrf_verify"
harness = false
```

- [ ] **Step 2: Write `crates/crypto/src/lib.rs`**

```rust
//! Cryptographic primitives for LUA-DAG.
//!
//! This crate wraps `blst` (BLS12-381) and `curve25519-dalek` (ECVRF on
//! Edwards25519) behind a small surface re-using the wire types from
//! `crates/types`. Aggregation, sortition, KDF, and a DKG fingerprint
//! stub live here as well.
#![cfg_attr(not(test), warn(missing_docs))]
// `blst` requires `unsafe` FFI calls. Override the workspace
// `unsafe_code = "forbid"` lint for this crate only.
#![allow(unsafe_code)]

pub mod bls;
pub mod dkg;
pub mod error;
pub mod hash;
pub mod kdf;
pub mod vrf;

pub use error::{Error, Result};
```

- [ ] **Step 3: Add the crate to workspace members**

Edit the workspace root `Cargo.toml`. Replace:

```toml
members = ["crates/types"]
```

with:

```toml
members = ["crates/types", "crates/crypto"]
```

- [ ] **Step 4: Verify it builds (will fail — modules missing)**

Run: `cargo build -p crypto`
Expected: FAIL on missing module files. Each subsequent task fixes one.

---

## Task 2: `error.rs` — crate error type

**Files:**
- Create: `crates/crypto/src/error.rs`

- [ ] **Step 1: Write the module + test**

```rust
//! Crate-level error type for cryptographic operations.

use thiserror::Error;

/// All failures from `crates/crypto`.
#[derive(Debug, Error)]
pub enum Error {
    /// BLS signature failed to verify.
    #[error("BLS signature verification failed")]
    BlsVerifyFailed,

    /// BLS aggregation could not combine the supplied signatures.
    #[error("BLS aggregation failed: {0}")]
    BlsAggregateFailed(&'static str),

    /// Proof-of-Possession invalid.
    #[error("Proof-of-Possession invalid")]
    PopInvalid,

    /// VRF proof failed to verify.
    #[error("VRF proof verification failed")]
    VrfVerifyFailed,

    /// Bitmap length disagrees with validator count.
    #[error("bitmap length mismatch: bitmap covers {bitmap_bits} bits, expected {expected}")]
    BitmapLength {
        /// Number of bits the supplied bitmap exposes.
        bitmap_bits: usize,
        /// Validator count the caller is operating over.
        expected: usize,
    },

    /// Stake-weighted sortition rejection (caller-loop should retry with next y_i).
    #[error("sortition rejection")]
    SortitionRejected,

    /// Encoding/decoding error from `types`.
    #[error("types codec error: {0}")]
    Types(#[from] types::Error),
}

/// Convenience result alias.
pub type Result<T> = core::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_length_error_shows_both_sides() {
        let e = Error::BitmapLength { bitmap_bits: 256, expected: 300 };
        let s = e.to_string();
        assert!(s.contains("256"));
        assert!(s.contains("300"));
    }
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p crypto --lib error::`
Expected: PASS (1 test).

---

## Task 3: `hash.rs` — Blake3 + SHA-256 + domain-separation tags

**Files:**
- Create: `crates/crypto/src/hash.rs`

- [ ] **Step 1: Write the module + tests**

```rust
//! Hashing primitives + global domain-separation tag (DST) registry.
//!
//! Every protocol message that gets hashed or signed picks one DST from
//! [`dst`]. New DSTs are appended; existing values must never change.

use sha2::{Digest, Sha256};
use types::crypto_types::Hash32;

/// Centralised DST registry.
///
/// `DST_*` constants are appended only; never edit an existing value or
/// the wire format changes.
pub mod dst {
    /// Generic content addressing inside `crates/types`.
    pub const CONTENT_HASH: &[u8] = b"lua-dag/v1/content";
    /// Bullshark MicroQc message.
    pub const MICRO_QC: &[u8] = b"lua-dag/v1/micro-qc";
    /// Macro proposal signing root.
    pub const MACRO_PROPOSAL: &[u8] = b"lua-dag/v1/macro-proposal";
    /// Macro vote signing root.
    pub const MACRO_VOTE: &[u8] = b"lua-dag/v1/macro-vote";
    /// Beacon chaining input.
    pub const BEACON: &[u8] = b"lua-dag/v1/beacon";
    /// Subnet membership derivation.
    pub const SUBNET_ASSIGN: &[u8] = b"lua-dag/v1/subnet-assign";
    /// Proof-of-Possession.
    pub const POP: &[u8] = b"lua-dag/v1/pop";
}

/// Blake3-256 over `data` with a DST prefix.
#[must_use]
pub fn blake3_with_dst(dst: &[u8], data: &[u8]) -> Hash32 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(dst);
    hasher.update(&[0x00]); // separator byte
    hasher.update(data);
    Hash32(*hasher.finalize().as_bytes())
}

/// SHA-256 over `data` with a DST prefix. Only for backwards-compat or
/// hash-to-curve flows that mandate SHA-256.
#[must_use]
pub fn sha256_with_dst(dst: &[u8], data: &[u8]) -> Hash32 {
    let mut hasher = Sha256::new();
    hasher.update(dst);
    hasher.update([0x00]);
    hasher.update(data);
    let out = hasher.finalize();
    let mut h = [0u8; 32];
    h.copy_from_slice(&out);
    Hash32(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn different_dsts_yield_different_hashes() {
        let a = blake3_with_dst(dst::MICRO_QC, b"hello");
        let b = blake3_with_dst(dst::MACRO_VOTE, b"hello");
        assert_ne!(a, b, "DST must change the hash output");
    }

    #[test]
    fn blake3_is_deterministic() {
        let a = blake3_with_dst(dst::BEACON, b"x");
        let b = blake3_with_dst(dst::BEACON, b"x");
        assert_eq!(a, b);
    }

    #[test]
    fn sha256_is_deterministic_and_differs_from_blake3() {
        let a = sha256_with_dst(dst::POP, b"x");
        let b = sha256_with_dst(dst::POP, b"x");
        let c = blake3_with_dst(dst::POP, b"x");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p crypto --lib hash::`
Expected: PASS (3 tests).

---

## Task 4: `bls/keys.rs` — SecretKey, PublicKey, Pop

**Files:**
- Create: `crates/crypto/src/bls/mod.rs`
- Create: `crates/crypto/src/bls/keys.rs`

- [ ] **Step 1: Write `crates/crypto/src/bls/mod.rs`**

```rust
//! BLS12-381 over `blst`.

pub mod aggregate;
pub mod bitmap;
pub mod keys;
pub mod sign;

pub use aggregate::{aggregate_sigs, verify_aggregate};
pub use bitmap::Bitmap;
pub use keys::{PublicKey, SecretKey, generate_pop, verify_pop};
pub use sign::{sign, verify};
```

- [ ] **Step 2: Write `crates/crypto/src/bls/keys.rs`**

```rust
//! BLS12-381 secret keys, public keys, and Proof-of-Possession.
//!
//! Wire shapes live in `types::crypto_types` — this module is responsible
//! for in-memory `blst` objects and the public-key boundary.

use blst::min_pk::{PublicKey as BlstPk, SecretKey as BlstSk, Signature as BlstSig};
use rand::RngCore;
use types::crypto_types::{BlsPubkey, BlsSig, Pop};

use crate::{
    error::{Error, Result},
    hash::dst,
};

/// BLS12-381 secret key wrapper.
///
/// Wraps `blst::min_pk::SecretKey` and zeroises on drop is **not** yet
/// implemented; consumers must keep keys in protected memory until that
/// is added (tracked separately).
#[derive(Clone, Debug)]
pub struct SecretKey(pub(crate) BlstSk);

/// BLS12-381 public key wrapper.
#[derive(Clone, Debug)]
pub struct PublicKey(pub(crate) BlstPk);

impl SecretKey {
    /// Generate a secret key from a 32-byte IKM (input keying material).
    ///
    /// Use [`SecretKey::random`] for ephemeral keys.
    pub fn from_ikm(ikm: &[u8; 32]) -> Result<Self> {
        let sk = BlstSk::key_gen(ikm, &[]).map_err(|_| Error::BlsAggregateFailed("key_gen"))?;
        Ok(Self(sk))
    }

    /// Generate a fresh secret key from the supplied RNG.
    pub fn random<R: RngCore>(rng: &mut R) -> Result<Self> {
        let mut ikm = [0u8; 32];
        rng.fill_bytes(&mut ikm);
        Self::from_ikm(&ikm)
    }

    /// Derive the matching public key.
    #[must_use]
    pub fn public(&self) -> PublicKey {
        PublicKey(self.0.sk_to_pk())
    }
}

impl PublicKey {
    /// Encode to the on-wire 48-byte compressed form.
    #[must_use]
    pub fn to_bytes(&self) -> BlsPubkey {
        BlsPubkey(self.0.compress())
    }

    /// Decode from on-wire 48-byte form. Verifies subgroup membership.
    pub fn from_bytes(b: &BlsPubkey) -> Result<Self> {
        BlstPk::uncompress(&b.0).map(Self).map_err(|_| Error::BlsVerifyFailed)
    }
}

/// Build a Proof-of-Possession for `sk` (`Sign(POP_DST, pk_bytes)`).
pub fn generate_pop(sk: &SecretKey) -> Pop {
    let pk_bytes = sk.public().to_bytes();
    let sig = sk.0.sign(&pk_bytes.0, dst::POP, &[]);
    Pop(BlsSig(sig.compress()))
}

/// Verify a Proof-of-Possession against the claimed public key.
pub fn verify_pop(pk: &PublicKey, pop: &Pop) -> Result<()> {
    let pk_bytes = pk.to_bytes();
    let sig = BlstSig::uncompress(&pop.0.0).map_err(|_| Error::PopInvalid)?;
    let err = sig.verify(true, &pk_bytes.0, dst::POP, &[], &pk.0, true);
    if err == blst::BLST_ERROR::BLST_SUCCESS {
        Ok(())
    } else {
        Err(Error::PopInvalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha20Rng;
    use rand::SeedableRng;

    #[test]
    fn keygen_and_pop_roundtrip() {
        let mut rng = ChaCha20Rng::from_seed([0; 32]);
        let sk = SecretKey::random(&mut rng).unwrap();
        let pk = sk.public();
        let pop = generate_pop(&sk);
        verify_pop(&pk, &pop).expect("PoP must verify");
    }

    #[test]
    fn public_key_round_trips_through_wire_form() {
        let mut rng = ChaCha20Rng::from_seed([1; 32]);
        let sk = SecretKey::random(&mut rng).unwrap();
        let pk = sk.public();
        let bytes = pk.to_bytes();
        let pk2 = PublicKey::from_bytes(&bytes).unwrap();
        assert_eq!(pk.to_bytes(), pk2.to_bytes());
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p crypto --lib bls::keys::`
Expected: PASS (2 tests).

---

## Task 5: `bls/sign.rs` — single-key sign + verify

**Files:**
- Create: `crates/crypto/src/bls/sign.rs`

- [ ] **Step 1: Write the module + tests**

```rust
//! Single-key BLS sign/verify. Aggregation is in `aggregate.rs`.

use blst::min_pk::Signature as BlstSig;
use types::crypto_types::BlsSig;

use super::keys::{PublicKey, SecretKey};
use crate::error::{Error, Result};

/// Sign `msg` with `sk` under the supplied DST.
#[must_use]
pub fn sign(sk: &SecretKey, dst: &[u8], msg: &[u8]) -> BlsSig {
    let sig = sk.0.sign(msg, dst, &[]);
    BlsSig(sig.compress())
}

/// Verify a single signature under DST.
pub fn verify(pk: &PublicKey, dst: &[u8], msg: &[u8], sig: &BlsSig) -> Result<()> {
    let s = BlstSig::uncompress(&sig.0).map_err(|_| Error::BlsVerifyFailed)?;
    let err = s.verify(true, msg, dst, &[], &pk.0, true);
    if err == blst::BLST_ERROR::BLST_SUCCESS {
        Ok(())
    } else {
        Err(Error::BlsVerifyFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bls::keys::SecretKey;
    use crate::hash::dst as dsts;
    use rand_chacha::ChaCha20Rng;
    use rand::SeedableRng;

    #[test]
    fn sign_then_verify_succeeds() {
        let mut rng = ChaCha20Rng::from_seed([2; 32]);
        let sk = SecretKey::random(&mut rng).unwrap();
        let pk = sk.public();
        let sig = sign(&sk, dsts::MICRO_QC, b"hello");
        verify(&pk, dsts::MICRO_QC, b"hello", &sig).unwrap();
    }

    #[test]
    fn wrong_message_fails() {
        let mut rng = ChaCha20Rng::from_seed([3; 32]);
        let sk = SecretKey::random(&mut rng).unwrap();
        let pk = sk.public();
        let sig = sign(&sk, dsts::MICRO_QC, b"hello");
        let err = verify(&pk, dsts::MICRO_QC, b"goodbye", &sig).unwrap_err();
        assert!(matches!(err, Error::BlsVerifyFailed));
    }

    #[test]
    fn wrong_dst_fails() {
        let mut rng = ChaCha20Rng::from_seed([4; 32]);
        let sk = SecretKey::random(&mut rng).unwrap();
        let pk = sk.public();
        let sig = sign(&sk, dsts::MICRO_QC, b"hello");
        let err = verify(&pk, dsts::MACRO_VOTE, b"hello", &sig).unwrap_err();
        assert!(matches!(err, Error::BlsVerifyFailed));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p crypto --lib bls::sign::`
Expected: PASS (3 tests).

---

## Task 6: `bls/aggregate.rs` — aggregate signatures + verify-aggregate

**Files:**
- Create: `crates/crypto/src/bls/aggregate.rs`

- [ ] **Step 1: Write the module + tests**

```rust
//! BLS aggregate signature helpers. Uses `blst::min_pk::AggregateSignature`.

use blst::min_pk::{AggregatePublicKey, AggregateSignature, Signature as BlstSig};
use types::crypto_types::BlsSig;

use super::keys::PublicKey;
use crate::error::{Error, Result};

/// Aggregate a slice of signatures into one compressed signature.
pub fn aggregate_sigs(sigs: &[BlsSig]) -> Result<BlsSig> {
    if sigs.is_empty() {
        return Err(Error::BlsAggregateFailed("empty signature set"));
    }
    let parsed: Vec<BlstSig> = sigs
        .iter()
        .map(|s| BlstSig::uncompress(&s.0).map_err(|_| Error::BlsAggregateFailed("invalid sig")))
        .collect::<Result<_>>()?;
    let refs: Vec<&BlstSig> = parsed.iter().collect();
    let agg = AggregateSignature::aggregate(&refs, true)
        .map_err(|_| Error::BlsAggregateFailed("aggregate"))?;
    Ok(BlsSig(agg.to_signature().compress()))
}

/// Verify that `agg` is the aggregate signature of `pks` over the same
/// `msg` under `dst`.
pub fn verify_aggregate(pks: &[PublicKey], dst: &[u8], msg: &[u8], agg: &BlsSig) -> Result<()> {
    if pks.is_empty() {
        return Err(Error::BlsAggregateFailed("empty pubkey set"));
    }
    let agg_sig = BlstSig::uncompress(&agg.0).map_err(|_| Error::BlsVerifyFailed)?;
    let pk_refs: Vec<&blst::min_pk::PublicKey> = pks.iter().map(|p| &p.0).collect();
    let agg_pk = AggregatePublicKey::aggregate(&pk_refs, true)
        .map_err(|_| Error::BlsAggregateFailed("aggregate pk"))?;
    let err = agg_sig.verify(true, msg, dst, &[], &agg_pk.to_public_key(), true);
    if err == blst::BLST_ERROR::BLST_SUCCESS {
        Ok(())
    } else {
        Err(Error::BlsVerifyFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bls::{keys::SecretKey, sign::sign};
    use crate::hash::dst as dsts;
    use rand_chacha::ChaCha20Rng;
    use rand::SeedableRng;

    #[test]
    fn aggregate_three_sigs_over_same_message() {
        let mut rng = ChaCha20Rng::from_seed([5; 32]);
        let sks: Vec<_> = (0..3).map(|_| SecretKey::random(&mut rng).unwrap()).collect();
        let pks: Vec<_> = sks.iter().map(SecretKey::public).collect();
        let msg = b"shared-message";
        let sigs: Vec<_> = sks.iter().map(|sk| sign(sk, dsts::MICRO_QC, msg)).collect();

        let agg = aggregate_sigs(&sigs).unwrap();
        verify_aggregate(&pks, dsts::MICRO_QC, msg, &agg).unwrap();
    }

    #[test]
    fn aggregate_with_wrong_pks_fails() {
        let mut rng = ChaCha20Rng::from_seed([6; 32]);
        let sks: Vec<_> = (0..3).map(|_| SecretKey::random(&mut rng).unwrap()).collect();
        let other = SecretKey::random(&mut rng).unwrap();
        let msg = b"m";
        let sigs: Vec<_> = sks.iter().map(|sk| sign(sk, dsts::MICRO_QC, msg)).collect();
        let agg = aggregate_sigs(&sigs).unwrap();

        let mut wrong_pks: Vec<_> = sks.iter().map(SecretKey::public).collect();
        wrong_pks[0] = other.public();
        let err = verify_aggregate(&wrong_pks, dsts::MICRO_QC, msg, &agg).unwrap_err();
        assert!(matches!(err, Error::BlsVerifyFailed));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p crypto --lib bls::aggregate::`
Expected: PASS (2 tests).

---

## Task 7: `bls/bitmap.rs` — signer bitmap helpers

**Files:**
- Create: `crates/crypto/src/bls/bitmap.rs`

- [ ] **Step 1: Write the module + tests**

```rust
//! Signer bitmaps for aggregated BLS certificates.
//!
//! Bitmap is little-endian-bit, big-endian-byte: bit `i` lives in
//! `bytes[i / 8] >> (i % 8)`. This matches Borsh-friendly slicing.

use crate::error::{Error, Result};

/// Mutable bitmap with a fixed number of slots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bitmap {
    bytes: Vec<u8>,
    bits:  usize,
}

impl Bitmap {
    /// New all-zero bitmap covering `bits` validators.
    #[must_use]
    pub fn new(bits: usize) -> Self {
        let len = (bits + 7) / 8;
        Self { bytes: vec![0u8; len], bits }
    }

    /// View raw bytes (length = ⌈bits/8⌉).
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Wrap an existing byte vector. `bits` must fit within the buffer.
    pub fn from_bytes(bytes: Vec<u8>, bits: usize) -> Result<Self> {
        let need = (bits + 7) / 8;
        if bytes.len() != need {
            return Err(Error::BitmapLength { bitmap_bits: bytes.len() * 8, expected: bits });
        }
        Ok(Self { bytes, bits })
    }

    /// Total bit count (validator count).
    #[must_use]
    pub fn len(&self) -> usize {
        self.bits
    }

    /// Number of bits set.
    #[must_use]
    pub fn count_ones(&self) -> usize {
        self.bytes.iter().map(|b| b.count_ones() as usize).sum()
    }

    /// Set bit `i`.
    pub fn set(&mut self, i: usize) -> Result<()> {
        if i >= self.bits {
            return Err(Error::BitmapLength { bitmap_bits: self.bits, expected: i + 1 });
        }
        self.bytes[i / 8] |= 1 << (i % 8);
        Ok(())
    }

    /// Test bit `i`.
    pub fn get(&self, i: usize) -> Result<bool> {
        if i >= self.bits {
            return Err(Error::BitmapLength { bitmap_bits: self.bits, expected: i + 1 });
        }
        Ok(self.bytes[i / 8] & (1 << (i % 8)) != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_round_trip() {
        let mut b = Bitmap::new(10);
        b.set(0).unwrap();
        b.set(7).unwrap();
        b.set(8).unwrap();
        assert!(b.get(0).unwrap());
        assert!(!b.get(1).unwrap());
        assert!(b.get(7).unwrap());
        assert!(b.get(8).unwrap());
        assert_eq!(b.count_ones(), 3);
    }

    #[test]
    fn out_of_range_errors() {
        let mut b = Bitmap::new(8);
        let err = b.set(8).unwrap_err();
        assert!(matches!(err, Error::BitmapLength { .. }));
    }

    #[test]
    fn from_bytes_validates_length() {
        let err = Bitmap::from_bytes(vec![0; 2], 24).unwrap_err();
        assert!(matches!(err, Error::BitmapLength { .. }));
        let ok = Bitmap::from_bytes(vec![0; 3], 24).unwrap();
        assert_eq!(ok.len(), 24);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p crypto --lib bls::bitmap::`
Expected: PASS (3 tests).

---

## Task 8: `vrf/ecvrf.rs` — Edwards25519 ECVRF prove/verify (RFC 9381)

This is a **placeholder implementation** for the skeleton phase: it uses Ed25519 deterministic signatures as the proof envelope so downstream code can plug in. Replacing this with full RFC 9381 ECVRF is tracked as a TODO in §13 of the spec (`vrf/ecvrf.rs`).

**Files:**
- Create: `crates/crypto/src/vrf/mod.rs`
- Create: `crates/crypto/src/vrf/ecvrf.rs`

- [ ] **Step 1: Write `crates/crypto/src/vrf/mod.rs`**

```rust
//! ECVRF (Edwards25519, RFC 9381) wrappers + stake-weighted sortition.

pub mod ecvrf;
pub mod sortition;

pub use ecvrf::{VrfKey, vrf_prove, vrf_verify};
pub use sortition::vrf_to_uniform;
```

- [ ] **Step 2: Write `crates/crypto/src/vrf/ecvrf.rs`**

```rust
//! Edwards25519 ECVRF, RFC 9381 (placeholder skeleton).
//!
//! The current implementation uses deterministic Ed25519 signatures from
//! `curve25519-dalek` as the proof envelope. This is **not** RFC 9381 —
//! see `docs/superpowers/specs/2026-05-11-folder-architecture-design.md`
//! §12 open question #3. The interface and byte shape (`VrfProof` is 80
//! bytes) match the eventual real ECVRF so call sites are stable.

use curve25519_dalek::{
    constants::ED25519_BASEPOINT_POINT, edwards::CompressedEdwardsY, scalar::Scalar,
};
use rand::RngCore;
use sha2::{Digest, Sha512};
use types::crypto_types::{Hash32, VrfProof};

use crate::error::{Error, Result};

/// VRF keypair.
#[derive(Clone, Debug)]
pub struct VrfKey {
    sk_scalar: Scalar,
    pk_compressed: CompressedEdwardsY,
}

impl VrfKey {
    /// Generate from RNG.
    pub fn random<R: RngCore>(rng: &mut R) -> Self {
        let mut seed = [0u8; 32];
        rng.fill_bytes(&mut seed);
        Self::from_seed(&seed)
    }

    /// Deterministic generation from a 32-byte seed.
    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        // Wide reduction so any 32-byte seed yields a valid scalar.
        let mut wide = [0u8; 64];
        wide[..32].copy_from_slice(seed);
        let sk_scalar = Scalar::from_bytes_mod_order_wide(&wide);
        let pk_point = ED25519_BASEPOINT_POINT * sk_scalar;
        Self { sk_scalar, pk_compressed: pk_point.compress() }
    }

    /// Compressed Edwards25519 public point.
    #[must_use]
    pub fn pubkey(&self) -> [u8; 32] {
        self.pk_compressed.to_bytes()
    }
}

/// Produce a VRF proof and its 32-byte output for `alpha`.
///
/// Output layout of the 80-byte proof:
///   bytes  0..32  = gamma (compressed point)
///   bytes 32..48  = c (truncated SHA-512 challenge)
///   bytes 48..80  = s (response scalar)
pub fn vrf_prove(key: &VrfKey, alpha: &[u8]) -> (VrfProof, Hash32) {
    // 1. Hash alpha to a scalar h.
    let h_scalar = hash_to_scalar(b"lua-dag/v1/vrf/h", alpha);
    // 2. gamma = h * sk
    let gamma_point = ED25519_BASEPOINT_POINT * (h_scalar * key.sk_scalar);
    let gamma = gamma_point.compress().to_bytes();
    // 3. k = nonce (deterministic — SHA-512(sk_bytes || alpha))
    let mut hasher = Sha512::new();
    hasher.update(b"lua-dag/v1/vrf/k");
    hasher.update(key.sk_scalar.to_bytes());
    hasher.update(alpha);
    let k_wide: [u8; 64] = hasher.finalize().into();
    let k = Scalar::from_bytes_mod_order_wide(&k_wide);
    // 4. Commitments: U = k*B, V = k*H (skeleton uses U only)
    let u_point = ED25519_BASEPOINT_POINT * k;
    let u_compressed = u_point.compress().to_bytes();
    // 5. c = H(pk || gamma || U)  (truncated to 16 bytes)
    let mut hasher = Sha512::new();
    hasher.update(b"lua-dag/v1/vrf/c");
    hasher.update(key.pubkey());
    hasher.update(gamma);
    hasher.update(u_compressed);
    let c_full: [u8; 64] = hasher.finalize().into();
    let mut c_bytes = [0u8; 16];
    c_bytes.copy_from_slice(&c_full[..16]);
    let mut c_scalar_bytes = [0u8; 32];
    c_scalar_bytes[..16].copy_from_slice(&c_bytes);
    let c_scalar = Scalar::from_bytes_mod_order(c_scalar_bytes);
    // 6. s = k - c * sk
    let s = k - c_scalar * key.sk_scalar;
    let s_bytes = s.to_bytes();
    // 7. Pack proof.
    let mut proof_bytes = [0u8; 80];
    proof_bytes[..32].copy_from_slice(&gamma);
    proof_bytes[32..48].copy_from_slice(&c_bytes);
    proof_bytes[48..80].copy_from_slice(&s_bytes);
    // 8. Output beta = H(gamma).
    let mut hasher = Sha512::new();
    hasher.update(b"lua-dag/v1/vrf/beta");
    hasher.update(gamma);
    let beta_full: [u8; 64] = hasher.finalize().into();
    let mut beta = [0u8; 32];
    beta.copy_from_slice(&beta_full[..32]);

    (VrfProof(proof_bytes), Hash32(beta))
}

/// Verify a VRF proof against `pk_bytes` (compressed Edwards25519 point)
/// and return the 32-byte output on success.
pub fn vrf_verify(pk_bytes: &[u8; 32], alpha: &[u8], proof: &VrfProof) -> Result<Hash32> {
    let pk = CompressedEdwardsY::from_slice(pk_bytes)
        .map_err(|_| Error::VrfVerifyFailed)?
        .decompress()
        .ok_or(Error::VrfVerifyFailed)?;
    let mut gamma_bytes = [0u8; 32];
    gamma_bytes.copy_from_slice(&proof.0[..32]);
    let gamma = CompressedEdwardsY::from_slice(&gamma_bytes)
        .map_err(|_| Error::VrfVerifyFailed)?
        .decompress()
        .ok_or(Error::VrfVerifyFailed)?;
    let mut c_bytes = [0u8; 16];
    c_bytes.copy_from_slice(&proof.0[32..48]);
    let mut s_bytes = [0u8; 32];
    s_bytes.copy_from_slice(&proof.0[48..80]);
    let s_scalar = Scalar::from_canonical_bytes(s_bytes)
        .into_option()
        .ok_or(Error::VrfVerifyFailed)?;
    let mut c_scalar_bytes = [0u8; 32];
    c_scalar_bytes[..16].copy_from_slice(&c_bytes);
    let c_scalar = Scalar::from_bytes_mod_order(c_scalar_bytes);
    // Reconstruct U = s*B + c*pk.
    let u_recovered = ED25519_BASEPOINT_POINT * s_scalar + pk * c_scalar;
    let u_compressed = u_recovered.compress().to_bytes();
    let mut hasher = Sha512::new();
    hasher.update(b"lua-dag/v1/vrf/c");
    hasher.update(pk_bytes);
    hasher.update(gamma.compress().to_bytes());
    hasher.update(u_compressed);
    let c_full: [u8; 64] = hasher.finalize().into();
    if &c_full[..16] != c_bytes.as_slice() {
        return Err(Error::VrfVerifyFailed);
    }
    let mut hasher = Sha512::new();
    hasher.update(b"lua-dag/v1/vrf/beta");
    hasher.update(gamma.compress().to_bytes());
    let beta_full: [u8; 64] = hasher.finalize().into();
    let mut beta = [0u8; 32];
    beta.copy_from_slice(&beta_full[..32]);
    Ok(Hash32(beta))
}

fn hash_to_scalar(dst: &[u8], data: &[u8]) -> Scalar {
    let mut hasher = Sha512::new();
    hasher.update(dst);
    hasher.update(data);
    let wide: [u8; 64] = hasher.finalize().into();
    Scalar::from_bytes_mod_order_wide(&wide)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha20Rng;
    use rand::SeedableRng;

    #[test]
    fn prove_then_verify_succeeds() {
        let mut rng = ChaCha20Rng::from_seed([7; 32]);
        let key = VrfKey::random(&mut rng);
        let (proof, beta) = vrf_prove(&key, b"alpha");
        let beta2 = vrf_verify(&key.pubkey(), b"alpha", &proof).unwrap();
        assert_eq!(beta, beta2);
    }

    #[test]
    fn wrong_alpha_fails() {
        let mut rng = ChaCha20Rng::from_seed([8; 32]);
        let key = VrfKey::random(&mut rng);
        let (proof, _) = vrf_prove(&key, b"alpha");
        let err = vrf_verify(&key.pubkey(), b"different", &proof).unwrap_err();
        assert!(matches!(err, Error::VrfVerifyFailed));
    }

    #[test]
    fn deterministic_proof_for_same_input() {
        let key = VrfKey::from_seed(&[9; 32]);
        let (proof1, beta1) = vrf_prove(&key, b"alpha");
        let (proof2, beta2) = vrf_prove(&key, b"alpha");
        assert_eq!(proof1.0, proof2.0);
        assert_eq!(beta1, beta2);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p crypto --lib vrf::ecvrf::`
Expected: PASS (3 tests).

---

## Task 9: `vrf/sortition.rs` — uniform mapping from VRF output

**Files:**
- Create: `crates/crypto/src/vrf/sortition.rs`

We expose only the helper `vrf_to_uniform`. The full stake-weighted selector is consensus-layer logic and lives in `consensus::leader::vrf_sortition` (plan 03).

- [ ] **Step 1: Write the module + tests**

```rust
//! Helpers that take a 32-byte VRF output and project it onto a uniform
//! `[0, 1)` value for stake-weighted sortition. The actual stake math
//! lives in `consensus::leader::vrf_sortition`.

use types::crypto_types::Hash32;

/// Map a 32-byte VRF output to a fraction in `[0, 1)`.
///
/// Uses the high 64 bits as a numerator over `2^64`; bias of `2^-64` is
/// negligible for our committee sizes.
#[must_use]
pub fn vrf_to_uniform(beta: &Hash32) -> f64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&beta.0[..8]);
    let n = u64::from_be_bytes(buf);
    (n as f64) / (u64::MAX as f64 + 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_maps_to_zero() {
        let u = vrf_to_uniform(&Hash32([0; 32]));
        assert!((u - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn all_ones_maps_close_to_one() {
        let u = vrf_to_uniform(&Hash32([0xFF; 32]));
        assert!(u > 0.999, "got {u}");
        assert!(u < 1.0, "must be < 1");
    }

    #[test]
    fn distinct_outputs_yield_distinct_uniforms() {
        let a = vrf_to_uniform(&Hash32([0xAA; 32]));
        let b = vrf_to_uniform(&Hash32([0xBB; 32]));
        assert!((a - b).abs() > 1e-9);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p crypto --lib vrf::sortition::`
Expected: PASS (3 tests).

---

## Task 10: `kdf.rs` — HKDF-Blake3 for beacon chaining + subnet assignment

**Files:**
- Create: `crates/crypto/src/kdf.rs`

- [ ] **Step 1: Write the module + tests**

```rust
//! HKDF-style key/value derivation using Blake3 keyed mode.

use blake3::Hasher;

/// HKDF-style expand using Blake3 keyed mode.
///
/// `ikm` is the input keying material (e.g. previous beacon output).
/// `info` is a per-call domain separator. Returns `len` bytes.
#[must_use]
pub fn expand(ikm: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    // Use Blake3 keyed-mode where the key is derived from the IKM, then
    // extend by absorbing `info` plus a counter.
    let key = blake3::hash(ikm);
    let mut out = Vec::with_capacity(len);
    let mut counter: u32 = 0;
    while out.len() < len {
        let mut hasher = Hasher::new_keyed(key.as_bytes());
        hasher.update(info);
        hasher.update(&counter.to_be_bytes());
        let block = hasher.finalize();
        out.extend_from_slice(block.as_bytes());
        counter += 1;
    }
    out.truncate(len);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_is_deterministic() {
        let a = expand(b"ikm", b"info", 64);
        let b = expand(b"ikm", b"info", 64);
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn different_info_yields_different_output() {
        let a = expand(b"ikm", b"alpha", 32);
        let b = expand(b"ikm", b"beta", 32);
        assert_ne!(a, b);
    }

    #[test]
    fn supports_long_outputs() {
        let out = expand(b"k", b"i", 200);
        assert_eq!(out.len(), 200);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p crypto --lib kdf::`
Expected: PASS (3 tests).

---

## Task 11: `dkg/fingerprint.rs` — DKG commitment fingerprint helper

**Files:**
- Create: `crates/crypto/src/dkg/mod.rs`
- Create: `crates/crypto/src/dkg/fingerprint.rs`

Full DKG is out of scope; this helper only computes a deterministic fingerprint over a `types::DkgCommitment` so storage can index it.

- [ ] **Step 1: Write `crates/crypto/src/dkg/mod.rs`**

```rust
//! DKG scaffolding.
//!
//! Full DKG ceremony is out of scope this phase (spec §2.2). This module
//! ships only a fingerprint helper used by storage/observability.

pub mod fingerprint;

pub use fingerprint::commitment_fingerprint;
```

- [ ] **Step 2: Write `crates/crypto/src/dkg/fingerprint.rs`**

```rust
//! Deterministic fingerprint over a `DkgCommitment`.

use types::{codec::canonical_hash, crypto_types::Hash32, validator::DkgCommitment};

use crate::error::Result;

/// Deterministic fingerprint suitable for storage indexing.
pub fn commitment_fingerprint(c: &DkgCommitment) -> Result<Hash32> {
    Ok(canonical_hash(c)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::{
        crypto_types::{BlsPubkey, Hash32},
        primitives::{Epoch, ValidatorId},
    };

    #[test]
    fn fingerprint_is_stable() {
        let c = DkgCommitment {
            validator: ValidatorId([1; 32]),
            epoch: Epoch(1),
            bls_pubkey: BlsPubkey([2; 48]),
            shares_root: Hash32([3; 32]),
        };
        let a = commitment_fingerprint(&c).unwrap();
        let b = commitment_fingerprint(&c).unwrap();
        assert_eq!(a, b);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p crypto --lib dkg::`
Expected: PASS (1 test).

---

## Task 12: Integration tests — `bls_pop.rs`, `vrf_determinism.rs`

**Files:**
- Create: `crates/crypto/tests/bls_pop.rs`
- Create: `crates/crypto/tests/vrf_determinism.rs`

- [ ] **Step 1: Write `crates/crypto/tests/bls_pop.rs`**

```rust
//! Proof-of-Possession across rotated keys.

use crypto::bls::{generate_pop, verify_pop, SecretKey};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

#[test]
fn cross_validator_pop_isolation() {
    let mut rng = ChaCha20Rng::from_seed([42; 32]);
    let sk_a = SecretKey::random(&mut rng).unwrap();
    let sk_b = SecretKey::random(&mut rng).unwrap();
    let pop_a = generate_pop(&sk_a);
    // A's PoP must verify under A's key but not under B's key.
    verify_pop(&sk_a.public(), &pop_a).expect("PoP must verify against owner");
    let err = verify_pop(&sk_b.public(), &pop_a).unwrap_err();
    assert!(matches!(err, crypto::Error::PopInvalid));
}
```

- [ ] **Step 2: Write `crates/crypto/tests/vrf_determinism.rs`**

```rust
//! VRF determinism + cross-key isolation.

use crypto::vrf::{vrf_prove, vrf_verify, VrfKey};

#[test]
fn same_alpha_same_key_yields_same_proof_and_output() {
    let key = VrfKey::from_seed(&[3; 32]);
    let (p1, b1) = vrf_prove(&key, b"window/0");
    let (p2, b2) = vrf_prove(&key, b"window/0");
    assert_eq!(p1.0, p2.0);
    assert_eq!(b1, b2);
}

#[test]
fn different_keys_yield_different_outputs_for_same_alpha() {
    let k1 = VrfKey::from_seed(&[1; 32]);
    let k2 = VrfKey::from_seed(&[2; 32]);
    let (_, b1) = vrf_prove(&k1, b"window/0");
    let (_, b2) = vrf_prove(&k2, b"window/0");
    assert_ne!(b1, b2);
}

#[test]
fn proof_verifies_only_against_own_pubkey() {
    let k1 = VrfKey::from_seed(&[1; 32]);
    let k2 = VrfKey::from_seed(&[2; 32]);
    let (proof, _) = vrf_prove(&k1, b"alpha");
    vrf_verify(&k1.pubkey(), b"alpha", &proof).expect("own key verifies");
    assert!(vrf_verify(&k2.pubkey(), b"alpha", &proof).is_err());
}
```

- [ ] **Step 3: Run integration tests**

Run: `cargo test -p crypto --tests`
Expected: PASS (4 tests).

---

## Task 13: Bench skeletons (criterion-free no-op benches so manifest is valid)

Real criterion benches land with the consensus crate (plan 03). For now, define `harness = false` benches as compiled-but-empty `main()` programs so `cargo build --benches` succeeds.

**Files:**
- Create: `crates/crypto/benches/bls_verify.rs`
- Create: `crates/crypto/benches/bls_aggregate.rs`
- Create: `crates/crypto/benches/vrf_verify.rs`

- [ ] **Step 1: Write each bench file with identical contents**

```rust
//! Placeholder bench harness. Real `criterion` benches arrive in plan 03.

fn main() {
    println!("bench placeholder — replaced in plan 03 with criterion harness");
}
```

(Repeat for all three files — copying the same body is intentional; each file is its own bench binary.)

- [ ] **Step 2: Verify they build**

Run: `cargo build -p crypto --benches`
Expected: exit 0.

---

## Task 14: Full crate lint + test + commit

- [ ] **Step 1: Full check**

Run sequentially:

```bash
cargo fmt -p crypto -- --check
cargo clippy -p crypto --all-targets -- -D warnings
cargo test -p crypto
cargo build -p crypto --benches
```

Expected: all four exit 0.

- [ ] **Step 2: Commit**

```bash
git add Cargo.toml crates/crypto/
git commit -m "feat(crypto): scaffold BLS, ECVRF, hashing, HKDF, DKG fingerprint"
```

---

## Self-Review

Spec coverage check against §7.2:

- `hash.rs`: ✅ Blake3 + SHA-256 + DST registry (Task 3).
- `bls/keys.rs`: ✅ `SecretKey`, `PublicKey`, `Pop`, `generate_pop`, `verify_pop` (Task 4).
- `bls/sign.rs`: ✅ single-key sign/verify (Task 5).
- `bls/aggregate.rs`: ✅ aggregate + verify_aggregate (Task 6).
- `bls/bitmap.rs`: ✅ `Bitmap` helpers (Task 7).
- `vrf/ecvrf.rs`: ✅ prove/verify (placeholder semantics; flagged in §12 open question #3 of the spec) (Task 8).
- `vrf/sortition.rs`: ✅ `vrf_to_uniform`; stake math intentionally deferred to `consensus::leader` (Task 9).
- `kdf.rs`: ✅ HKDF-Blake3 expand (Task 10).
- `dkg/fingerprint.rs`: ✅ deterministic commitment fingerprint (Task 11).
- `benches/`: ✅ three placeholders so the Cargo manifest is valid (Task 13).
- `tests/bls_pop.rs` + `tests/vrf_determinism.rs`: ✅ Task 12.
- Spec line "Public API ẩn lib cụ thể sau trait alias mỏng": addressed by re-exporting only typed shapes (`SecretKey`, `PublicKey`, `VrfKey`); future PQ swap touches only this crate.

Naming consistency with plan 01: `Pop` (Rust-friendly capitalisation) matches `types::crypto_types::Pop`. `VrfProof` is the 80-byte tuple from `types`. All signatures use `&BlsSig` / `BlsPubkey` boundaries; internal `blst` types stay private (`pub(crate)`).

No placeholders unmarked — the ECVRF placeholder is explicitly documented and the spec already lists the trade-off in §12.
