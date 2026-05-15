//! Bootstrap the starting validator set + react to epoch transitions.
//!
//! Real bootstrap mechanism (file? RPC? genesis dump?) is open in the
//! spec; this skeleton loads from a TOML file.

use std::path::Path;

use anyhow::Result;
use types::validator::ValidatorSet;

/// Read a validator set from a TOML file.
#[allow(dead_code)]
pub fn load_from_toml(path: &Path) -> Result<ValidatorSet> {
    let raw = std::fs::read_to_string(path)?;
    let set: ValidatorSet = toml::from_str(&raw)?;
    Ok(set)
}
