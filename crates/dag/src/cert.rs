//! Quorum BLS certificates for [`CertifiedVertex`].

use crypto::{
    bls::{
        aggregate::{aggregate_sigs, verify_aggregate},
        bitmap::Bitmap,
        keys::{PublicKey, SecretKey},
        sign::sign,
    },
    hash::dst,
};
use types::{
    crypto_types::BlsAggSig,
    dag::{CertifiedVertex, Vertex},
    validator::ValidatorSet,
};

use crate::{devnet, signing};

#[derive(Debug, thiserror::Error)]
pub enum CertError {
    #[error("vertex content hash mismatch")]
    HashMismatch,
    #[error("insufficient signers: got {got}, need {need}")]
    InsufficientSigners { got: u32, need: u32 },
    #[error("validator index out of range: {0}")]
    BadIndex(u32),
    #[error("unknown devnet author")]
    UnknownDevnetAuthor,
    #[error("bls: {0}")]
    Bls(#[from] crypto::error::Error),
}

pub type Result<T> = std::result::Result<T, CertError>;

fn quorum_threshold(n: u32) -> u32 {
    let f = n.saturating_sub(1) / 3;
    2 * f + 1
}

/// Build a quorum certificate over `vertex` from `signer_indices` (valset positions).
pub fn build_quorum_cert(
    vertex: &Vertex,
    valset: &ValidatorSet,
    signer_indices: &[u32],
) -> Result<CertifiedVertex> {
    build_quorum_cert_with(vertex, valset, signer_indices, |idx| {
        let entry = valset
            .entries
            .get(idx as usize)
            .ok_or(CertError::BadIndex(idx))?;
        let label = devnet::devnet_label_for_validator_id(&entry.id)
            .ok_or(CertError::UnknownDevnetAuthor)?;
        SecretKey::from_ikm(&devnet::devnet_bls_ikm(label)).map_err(CertError::Bls)
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
    let n = u32::try_from(valset.entries.len()).map_err(|_| CertError::BadIndex(0))?;
    let need = quorum_threshold(n);
    if u32::try_from(signer_indices.len()).unwrap_or(0) < need {
        return Err(CertError::InsufficientSigners {
            got: u32::try_from(signer_indices.len()).unwrap_or(0),
            need,
        });
    }
    let msg = signing::signing_bytes(vertex);
    let mut sigs = Vec::with_capacity(signer_indices.len());
    let mut contributors = Vec::with_capacity(signer_indices.len());
    for &idx in signer_indices {
        if valset.entries.get(idx as usize).is_none() {
            return Err(CertError::BadIndex(idx));
        }
        let sk = sk_at(idx)?;
        sigs.push(sign(&sk, dst::VERTEX_CERT, &msg));
        contributors.push(idx);
    }
    let agg = aggregate_sigs(&sigs).map_err(CertError::Bls)?;
    let mut bm = Bitmap::new(n as usize);
    for &idx in &contributors {
        bm.set(idx as usize).map_err(|_| CertError::BadIndex(idx))?;
    }
    Ok(CertifiedVertex {
        vertex: vertex.clone(),
        certificate: BlsAggSig {
            sig: agg,
            bitmap: bm.as_bytes().to_vec(),
        },
    })
}

fn bitmap_indices(bitmap: &[u8], n: u32) -> Result<Vec<u32>> {
    let bm = Bitmap::from_bytes(bitmap.to_vec(), n as usize)
        .map_err(|_| CertError::BadIndex(0))?;
    let mut out = Vec::new();
    for i in 0..n as usize {
        if bm.get(i).map_err(|_| CertError::BadIndex(i as u32))? {
            out.push(i as u32);
        }
    }
    Ok(out)
}

/// Verify content hash + quorum BLS certificate.
pub fn verify_certified_vertex(cv: &CertifiedVertex, valset: &ValidatorSet) -> Result<()> {
    if cv.vertex.hash != signing::content_hash(&cv.vertex) {
        return Err(CertError::HashMismatch);
    }
    let n = u32::try_from(valset.entries.len()).map_err(|_| CertError::BadIndex(0))?;
    let need = quorum_threshold(n);
    let indices = bitmap_indices(&cv.certificate.bitmap, n)?;
    if u32::try_from(indices.len()).unwrap_or(0) < need {
        return Err(CertError::InsufficientSigners {
            got: u32::try_from(indices.len()).unwrap_or(0),
            need,
        });
    }
    let msg = signing::signing_bytes(&cv.vertex);
    let pks: Vec<PublicKey> = indices
        .iter()
        .map(|&idx| {
            let entry = valset
                .entries
                .get(idx as usize)
                .ok_or(CertError::BadIndex(idx))?;
            PublicKey::from_bytes(&entry.bls_pubkey).map_err(CertError::Bls)
        })
        .collect::<Result<_>>()?;
    verify_aggregate(&pks, dst::VERTEX_CERT, &msg, &cv.certificate.sig).map_err(CertError::Bls)?;
    Ok(())
}
