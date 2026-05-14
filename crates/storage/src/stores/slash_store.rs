//! Append-only slashing evidence log keyed by monotonic sequence.

use types::slashing::SlashEvidence;

use crate::{
    columns::ColumnFamily,
    db::Database,
    error::{Error, Result},
};

/// Append evidence. The caller passes the next sequence number.
pub fn append(db: &Database, seq: u64, ev: &SlashEvidence) -> Result<()> {
    let key = seq.to_be_bytes();
    let bytes = borsh::to_vec(ev).map_err(|e| Error::Codec(e.to_string()))?;
    db.put_raw(ColumnFamily::SlashEvidence, &key, &bytes)
}
