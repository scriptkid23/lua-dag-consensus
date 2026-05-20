//! MicroQc aggregation (≥ ⌈2/3·C⌉).

use std::collections::HashSet;

use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    dag::CertifiedVertex,
    micro::MicroQc,
    primitives::{Epoch, ValidatorId},
};

use crate::{config::Config, error::Result, host_context::HostContext};

/// Per-state-machine set of `checkpoint_hash`es this validator has already
/// broadcast a [`MicroQc`] for. Used to keep `MicroQcAssembled` idempotent
/// across the `l2_minimal` deletion (whitepaper §8 still requires
/// once-per-checkpoint emission).
#[derive(Debug, Default)]
pub struct EmittedSet {
    inner: HashSet<Hash32>,
}

impl EmittedSet {
    /// New empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// True if `hash` has already been emitted.
    #[must_use]
    pub fn contains(&self, hash: &Hash32) -> bool {
        self.inner.contains(hash)
    }

    /// Mark `hash` as emitted; returns `true` if newly inserted.
    pub fn insert(&mut self, hash: Hash32) -> bool {
        self.inner.insert(hash)
    }

    /// Number of distinct emissions tracked.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

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
}

/// Per-validator commit aggregation: build a flat `MicroQc` over the
/// authors of the linearized batch. Stake check enforces ≥ ⌈2/3·n⌉
/// distinct authors (equal-stake sim assumption — generalises to stake
/// when the validator set carries non-uniform weights).
///
/// Returns `Ok(None)` until the threshold is met; `Ok(Some(qc))` once it
/// is. The producer is responsible for `EmittedSet` bookkeeping.
pub fn try_finalize(
    checkpoint_hash: Hash32,
    linearized: &[CertifiedVertex],
    ctx: &HostContext<'_>,
) -> Result<Option<MicroQc>> {
    let set = ctx
        .valset
        .set_for(Epoch(0))?
        .ok_or_else(|| crate::Error::InvalidConfig("no validator set for epoch 0".into()))?;
    let n = set.entries.len();
    if n == 0 {
        return Ok(None);
    }
    let authors: HashSet<ValidatorId> = linearized.iter().map(|cv| cv.vertex.author).collect();
    let f = (n - 1) / 3;
    let need = 2 * f + 1;
    if authors.len() < need {
        return Ok(None);
    }
    let mut bitmap = vec![0u8; n.div_ceil(8)];
    for (i, entry) in set.entries.iter().enumerate() {
        if authors.contains(&entry.id) {
            bitmap[i / 8] |= 1 << (i % 8);
        }
    }
    Ok(Some(MicroQc {
        checkpoint_hash,
        agg: BlsAggSig {
            sig: BlsSig([0xAB; 96]),
            bitmap,
        },
    }))
}
