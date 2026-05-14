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
