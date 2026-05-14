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
