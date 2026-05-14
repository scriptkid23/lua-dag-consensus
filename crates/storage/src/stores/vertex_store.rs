//! `(round, author) -> CertifiedVertex`.

use types::dag::CertifiedVertex;

use crate::{
    columns::ColumnFamily,
    db::Database,
    error::{Error, Result},
    keys,
};

/// Store a certified vertex.
pub fn put(db: &Database, v: &CertifiedVertex) -> Result<()> {
    let key = keys::vertex(v.vertex.round, &v.vertex.author);
    let bytes = borsh::to_vec(v).map_err(|e| Error::Codec(e.to_string()))?;
    db.put_raw(ColumnFamily::Vertex, &key, &bytes)
}

/// Fetch by `(round, author)` key.
pub fn get(
    db: &Database,
    round: types::primitives::Round,
    author: &types::primitives::ValidatorId,
) -> Result<Option<CertifiedVertex>> {
    let key = keys::vertex(round, author);
    match db.get_raw(ColumnFamily::Vertex, &key)? {
        Some(bytes) => borsh::from_slice(&bytes)
            .map(Some)
            .map_err(|e| Error::Codec(e.to_string())),
        None => Ok(None),
    }
}
