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
