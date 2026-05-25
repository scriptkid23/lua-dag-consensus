use dag::da::{
    challenge::{verify_availability_response, AvailabilityChallenge, AvailabilityResponse},
    ChallengeError,
};
use dag::erasure::{encode_shards, rs_merkle_commitment, ErasureConfig};
use types::{dag::BlobRef, primitives::{BlobId, ValidatorId}};

#[test]
fn honest_availability_response_passes_verify_hook() {
    let cfg = ErasureConfig::devnet_default();
    let payload = vec![0xABu8; 100_000];
    let shards = encode_shards(&payload, &cfg).unwrap();
    let commitment = rs_merkle_commitment(&shards);
    let blob_id = BlobId([9; 32]);
    let blob_ref = BlobRef {
        blob_id,
        commitment,
        size_bytes: u64::try_from(payload.len()).unwrap(),
    };
    let challenge = AvailabilityChallenge {
        blob_id,
        shard_indices: vec![0, 1, 2, 3],
        challenger: ValidatorId([1; 32]),
    };
    let response = AvailabilityResponse {
        blob_id,
        shards: shards
            .into_iter()
            .enumerate()
            .take(4)
            .map(|(i, data)| (u32::try_from(i).unwrap(), data))
            .collect(),
    };
    verify_availability_response(&challenge, &response, commitment, &blob_ref, &cfg).unwrap();
}

#[test]
fn tampered_shard_fails_verify_hook() {
    let cfg = ErasureConfig::devnet_default();
    let payload = vec![0xABu8; 100_000];
    let mut shards = encode_shards(&payload, &cfg).unwrap();
    let commitment = rs_merkle_commitment(&shards);
    let blob_id = BlobId([8; 32]);
    let blob_ref = BlobRef {
        blob_id,
        commitment,
        size_bytes: u64::try_from(payload.len()).unwrap(),
    };
    shards[0][0] ^= 0x01;
    let challenge = AvailabilityChallenge {
        blob_id,
        shard_indices: vec![0, 1, 2, 3],
        challenger: ValidatorId([2; 32]),
    };
    let response = AvailabilityResponse {
        blob_id,
        shards: shards
            .into_iter()
            .enumerate()
            .take(4)
            .map(|(i, data)| (u32::try_from(i).unwrap(), data))
            .collect(),
    };
    let err = verify_availability_response(&challenge, &response, commitment, &blob_ref, &cfg)
        .unwrap_err();
    assert!(matches!(err, ChallengeError::Commitment(_)));
}
