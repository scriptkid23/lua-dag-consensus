//! In-memory validator set port.

use consensus::ports::validator_set::ValidatorSetPort;
use types::{
    primitives::{Epoch, ValidatorId},
    validator::ValidatorSet,
};

/// Single-epoch validator set holder.
#[derive(Debug)]
pub struct VirtualValidatorSet {
    set: ValidatorSet,
}

impl VirtualValidatorSet {
    /// Wrap a `ValidatorSet`.
    #[must_use]
    pub fn new(set: ValidatorSet) -> Self {
        Self { set }
    }
}

impl ValidatorSetPort for VirtualValidatorSet {
    fn set_for(&self, epoch: Epoch) -> consensus::Result<Option<ValidatorSet>> {
        if self.set.epoch == epoch {
            Ok(Some(self.set.clone()))
        } else {
            Ok(None)
        }
    }

    fn index_of(&self, epoch: Epoch, validator: &ValidatorId) -> consensus::Result<Option<u32>> {
        if self.set.epoch != epoch {
            return Ok(None);
        }
        Ok(self
            .set
            .entries
            .iter()
            .position(|e| &e.id == validator)
            .map(|i| u32::try_from(i).unwrap_or(u32::MAX)))
    }
}
