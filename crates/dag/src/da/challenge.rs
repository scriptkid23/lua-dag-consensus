use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;
use types::{crypto_types::Hash32, dag::BlobRef, primitives::{BlobId, ValidatorId}};

use crate::erasure::{decode_shards, encode_shards, rs_merkle_commitment, ErasureConfig};

/// Challenge for shard indices of a blob (wire skeleton; no slash emission).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct AvailabilityChallenge {
    /// Target blob.
    pub blob_id: BlobId,
    /// Requested shard indices.
    pub shard_indices: Vec<u32>,
    /// Challenger validator id.
    pub challenger: ValidatorId,
}

/// Response carrying shard bytes for challenged indices.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct AvailabilityResponse {
    /// Target blob.
    pub blob_id: BlobId,
    /// `(index, shard_bytes)` pairs.
    pub shards: Vec<(u32, Vec<u8>)>,
}

/// Verify hook failures (no slash side effects).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChallengeError {
    /// Indices or blob id mismatch.
    #[error("challenge mismatch: {0}")]
    Mismatch(&'static str),
    /// Decode / commitment check failed.
    #[error("commitment verify failed: {0}")]
    Commitment(String),
}

pub type Result<T> = core::result::Result<T, ChallengeError>;

/// Verify a DA response against the challenged indices and expected RS commitment.
pub fn verify_availability_response(
    challenge: &AvailabilityChallenge,
    response: &AvailabilityResponse,
    expected_commitment: Hash32,
    blob_ref: &BlobRef,
    cfg: &ErasureConfig,
) -> Result<()> {
    if response.blob_id != challenge.blob_id {
        return Err(ChallengeError::Mismatch("blob_id"));
    }

    let mut resp_indices: Vec<u32> = response.shards.iter().map(|(i, _)| *i).collect();
    resp_indices.sort_unstable();
    let mut want = challenge.shard_indices.clone();
    want.sort_unstable();
    if resp_indices != want {
        return Err(ChallengeError::Mismatch("shard_indices"));
    }

    if response.shards.len() < usize::try_from(cfg.k).expect("k fits usize") {
        return Ok(());
    }

    let decoded = decode_shards(&response.shards, cfg, blob_ref.size_bytes as usize)
        .map_err(|e| ChallengeError::Commitment(e.to_string()))?;
    let full_shards = encode_shards(&decoded, cfg)
        .map_err(|e| ChallengeError::Commitment(e.to_string()))?;
    let recomputed = rs_merkle_commitment(&full_shards);
    if recomputed != expected_commitment {
        return Err(ChallengeError::Commitment(
            "recomputed merkle root mismatch".into(),
        ));
    }

    for (index, data) in &response.shards {
        if full_shards[*index as usize] != *data {
            return Err(ChallengeError::Commitment(format!(
                "tampered shard at index {index}"
            )));
        }
    }

    Ok(())
}
